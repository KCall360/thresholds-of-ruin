//! Structural layers must not hide the exposed face of sewn edges or straps.
//! These fixtures use spawn (including native smoothing and child transforms),
//! then remove the accessory to measure the unchanged underlying triangles.
use super::*;
use bevy::render::mesh::VertexAttributeValues;
use std::collections::{BTreeMap, BTreeSet};

const MARKER: Vec3 = Vec3::new(0.79, 0.07, 0.63);

#[derive(Clone, Copy)]
struct Triangle {
    p: [Vec3; 3],
    colors: [Vec3; 3],
    metal: bool,
}

impl Triangle {
    fn signature(self) -> [[i32; 3]; 3] {
        let mut points = self
            .p
            .map(|p| p.to_array().map(|v| (v * 1e6).round() as i32));
        points.sort();
        points
    }

    fn painted(self, color: Vec3) -> bool {
        !self.metal && self.colors.iter().all(|c| c.distance(color) < 1e-5)
    }

    fn center(self) -> Vec3 {
        (self.p[0] + self.p[1] + self.p[2]) / 3.0
    }

    // Intersect the actual triangle with a front/back viewing ray. This does
    // not reuse the SDF or the attachment solver's support approximation.
    fn depth(self, at: Vec2) -> Option<f32> {
        self.depth_inside(at, -1e-5)
    }

    fn depth_inside(self, at: Vec2, margin: f32) -> Option<f32> {
        let [a, b, c] = self.p;
        let ab = (b - a).truncate();
        let ac = (c - a).truncate();
        let q = at - a.truncate();
        let det = ab.perp_dot(ac);
        if det.abs() < 1e-12 {
            return None;
        }
        let u = q.perp_dot(ac) / det;
        let v = ab.perp_dot(q) / det;
        (u >= margin && v >= margin && u + v <= 1.0 - margin)
            .then_some(a.z + u * (b.z - a.z) + v * (c.z - a.z))
    }
}

fn linear(color: Vec3) -> Vec3 {
    let c = Color::srgb(color.x, color.y, color.z).to_linear();
    Vec3::new(c.red, c.green, c.blue)
}

fn female(kind: CreatureModel, build: u8) -> Look {
    Look::with_choice(
        kind,
        42,
        Some(Variant {
            woman: true,
            build,
            ..default()
        }),
    )
}

fn torso(look: &Look) -> Vec<Triangle> {
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let metal = library.materials.metal().id();
    let mut meshes = Assets::<Mesh>::default();
    let mut world = World::new();
    let parent = world.spawn(Transform::default()).id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = spawn(
        &mut Commands::new(&mut queue, &world),
        &mut library,
        &mut meshes,
        parent,
        look,
    );
    queue.apply(&mut world);
    let mut triangles = Vec::new();
    for child in world
        .get::<Children>(rig.joint(J::Torso).unwrap())
        .unwrap()
        .iter()
    {
        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("torso positions")
        };
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("torso pigments")
        };
        let transform = world.get::<Transform>(child).unwrap().compute_affine();
        let is_metal = world
            .get::<MeshMaterial3d<StandardMaterial>>(child)
            .unwrap()
            .0
            .id()
            == metal;
        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
        for index in indices.as_chunks::<3>().0 {
            triangles.push(Triangle {
                p: index.map(|i| transform.transform_point3(Vec3::from_array(points[i]))),
                colors: index.map(|i| Vec3::new(colors[i][0], colors[i][1], colors[i][2])),
                metal: is_metal,
            });
        }
    }
    assert!(!triangles.is_empty(), "missing spawned torso");
    triangles
}

// XY bins keep the comparison bounded without reducing the sampled triangles.
struct Support {
    triangles: Vec<Triangle>,
    bins: BTreeMap<(i32, i32), Vec<usize>>,
}

#[test]
fn ranger_cloak_covers_the_rear_baldric_without_an_abrupt_visible_cutoff() {
    for model in [
        CreatureModel::Vampire,
        CreatureModel::Zombie,
        CreatureModel::Troll,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let mut look = Look::with_choice(
                    model,
                    42,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                )
                .dressed(Wardrobe::Ranger);
                look.outfit
                    .as_mut()
                    .unwrap()
                    .baldric
                    .as_mut()
                    .unwrap()
                    .color = MARKER;
                let strap: Vec<_> = torso(&look)
                    .into_iter()
                    .filter(|t| t.painted(linear(MARKER)) && t.center().z < -0.03)
                    .collect();
                assert!(!strap.is_empty(), "rear strap witnesses");
                let (world, rig, _, meshes) = crate::creatures::rogue_tests::spawn_one(&look);
                let entity = rig
                    .drapes
                    .iter()
                    .find(|(_, kind)| *kind == Drape::Cloak)
                    .unwrap()
                    .0;
                let mesh = meshes.get(&world.get::<Mesh3d>(entity).unwrap().0).unwrap();
                let Some(VertexAttributeValues::Float32x3(points)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("cloak positions")
                };
                let transform = world.get::<Transform>(entity).unwrap().compute_affine();
                let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                let cloth = Support::new(
                    indices
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .map(|tri| Triangle {
                            p: tri.map(|i| transform.transform_point3(Vec3::from_array(points[i]))),
                            colors: [Vec3::ZERO; 3],
                            metal: false,
                        })
                        .collect(),
                );
                let mut checked = 0;
                for triangle in strap {
                    let at = triangle.center();
                    if let Some(inside) = cloth.depth(at, true) {
                        checked += 1;
                        assert!(inside < at.z - 0.002, "{model:?} woman={woman} build={build}: rear strap pierces cloak at {at:?}, lining depth {inside}");
                    }
                }
                assert!(
                    checked > 30,
                    "substantial rear strap must lie beneath the cloak: {checked}"
                );
            }
        }
    }
}

impl Support {
    fn cell(v: f32) -> i32 {
        (v / 0.02).floor() as i32
    }

    fn new(triangles: Vec<Triangle>) -> Self {
        let mut bins: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (i, tri) in triangles.iter().enumerate() {
            let lo = tri.p[0].min(tri.p[1]).min(tri.p[2]);
            let hi = tri.p[0].max(tri.p[1]).max(tri.p[2]);
            for x in Self::cell(lo.x)..=Self::cell(hi.x) {
                for y in Self::cell(lo.y)..=Self::cell(hi.y) {
                    bins.entry((x, y)).or_default().push(i);
                }
            }
        }
        Self { triangles, bins }
    }

    fn depth(&self, p: Vec3, front: bool) -> Option<f32> {
        self.bins
            .get(&(Self::cell(p.x), Self::cell(p.y)))?
            .iter()
            .filter_map(|i| self.triangles[*i].depth(p.truncate()))
            .reduce(|a, b| if front { a.max(b) } else { a.min(b) })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Coverage {
    Absent,
    Bilateral,
    SingleStrip,
}

#[derive(Clone, Copy)]
struct Band {
    name: &'static str,
    low: f32,
    high: f32,
    back: bool,
    coverage: Coverage,
}

/// Restrict the support comparison to geometry that really is unchanged by
/// suppressing the ornament. Closing Medusa's neckline changes paint, not the
/// body mesh. Selecting Orc's native recipe removes its decorative cowl; its
/// extra neck cut is above the short shell. Verify that premise explicitly.
fn check_visibility(
    label: &str,
    look: &Look,
    suppressed: &Look,
    pigment: Vec3,
    bands: &[Band],
) -> Vec<String> {
    let visible = torso(look);
    let baseline = torso(suppressed);
    let keys: BTreeSet<_> = visible.iter().map(|t| t.signature()).collect();
    let low = bands.iter().map(|b| b.low).reduce(f32::min).unwrap();
    let high = bands.iter().map(|b| b.high).reduce(f32::max).unwrap();
    let relevant: Vec<_> = baseline
        .into_iter()
        .filter(|t| {
            let lo = t.p[0].y.min(t.p[1].y).min(t.p[2].y);
            let hi = t.p[0].y.max(t.p[1].y).max(t.p[2].y);
            hi >= low && lo <= high
        })
        .collect();
    assert!(!relevant.is_empty(), "{label}: missing structural baseline");
    let unmatched = relevant
        .iter()
        .filter(|t| !keys.contains(&t.signature()))
        .count();
    assert_eq!(
        unmatched, 0,
        "{label}: suppression changed structural geometry in the measured height range"
    );
    let pigment = linear(pigment);
    assert!(
        !relevant.iter().any(|t| t.painted(pigment)),
        "{label}: pigment still exists with target suppressed"
    );
    let support = Support::new(relevant);
    // Back and edge faces have a darker pigment. Requiring all three exact
    // linear vertex colors selects only the authored outer ribbon surface.
    let tops: Vec<_> = visible.into_iter().filter(|t| t.painted(pigment)).collect();
    assert!(!tops.is_empty(), "{label}: missing accessory top faces");
    let top_surface = Support::new(tops.clone());
    let mut failures = Vec::new();
    for band in bands {
        if band.coverage == Coverage::Absent {
            // The authored lower armhole path is outside the waist silhouette.
            // Binding must end where its path leaves the garment, not continue
            // floating in space or pull a new strip of body out to support it.
            // This is an absence contract, not a demand for fabricated support.
            let floating = tops
                .iter()
                .filter(|tri| {
                    let lo = tri.p[0].y.min(tri.p[1].y).min(tri.p[2].y);
                    let hi = tri.p[0].y.max(tri.p[1].y).max(tri.p[2].y);
                    hi > band.low && lo < band.high
                })
                .count();
            if floating != 0 {
                failures.push(format!("{label} {}: {floating} top triangles remain in the unsupported lower binding region", band.name));
            }
            continue;
        }
        for front in [true, false]
            .into_iter()
            .filter(|front| *front || band.back)
        {
            let mut count = 0;
            let mut hits = 0;
            let mut clear = 0;
            let mut left = 0;
            let mut right = 0;
            let mut folded = 0;
            let mut inverted = 0;
            let mut worst = (f32::INFINITY, Vec3::ZERO);
            for tri in &tops {
                let normal = (tri.p[1] - tri.p[0]).cross(tri.p[2] - tri.p[0]);
                let center = tri.center();
                if center.y >= band.low && center.y <= band.high {
                    // Determine the outward side independently from winding.
                    // A reversed top face beyond either structural surface is
                    // culled even though its position alone would be clear.
                    let outer = support.depth(center, front);
                    if outer.is_some_and(|z| if front { center.z > z } else { center.z < z })
                        && (normal.z > 0.0) != front
                    {
                        inverted += 1;
                    }
                }
                if normal.z.abs() < 1e-12 || (normal.z > 0.0) != front {
                    continue;
                }
                // A fixed XY ray must cross one outward top per strip. More
                // than one interior intersection reveals folded/overlapping
                // rows. Skip the neckline/armhole junction, where two distinct
                // sewn edges intentionally meet; lower armholes and harnesses
                // remain independent single-strip witnesses.
                if center.y >= band.low
                    && center.y <= band.high
                    && (band.coverage == Coverage::SingleStrip || band.high < 0.4)
                {
                    let intersections = top_surface
                        .bins
                        .get(&(Support::cell(center.x), Support::cell(center.y)))
                        .into_iter()
                        .flatten()
                        .filter(|i| {
                            let other = top_surface.triangles[**i];
                            let n = (other.p[1] - other.p[0]).cross(other.p[2] - other.p[0]);
                            (n.z > 0.0) == front
                                && other.depth_inside(center.truncate(), 1e-4).is_some()
                        })
                        .count();
                    folded += usize::from(intersections > 1);
                }
                // Interiors test the rendered faces, including interpolation
                // between attachment samples; boundary-only tests miss burial.
                for p in [
                    tri.center(),
                    tri.p[0] * 0.6 + tri.p[1] * 0.2 + tri.p[2] * 0.2,
                    tri.p[1] * 0.6 + tri.p[0] * 0.2 + tri.p[2] * 0.2,
                    tri.p[2] * 0.6 + tri.p[0] * 0.2 + tri.p[1] * 0.2,
                ] {
                    if p.y < band.low || p.y > band.high {
                        continue;
                    }
                    count += 1;
                    let Some(z) = support.depth(p, front) else {
                        continue;
                    };
                    hits += 1;
                    left += usize::from(p.x < 0.0);
                    right += usize::from(p.x >= 0.0);
                    let gap = (p.z - z) * if front { 1.0 } else { -1.0 };
                    if gap < worst.0 {
                        worst = (gap, p);
                    }
                    clear += usize::from(gap >= 0.0001);
                }
            }
            // A small positive separation is required, not merely a matching
            // vertex or a sliver visible at the edge of an otherwise buried band.
            // Missing ray support is also reported instead of silently dropped.
            let missing_side = band.coverage == Coverage::Bilateral && (left < 4 || right < 4);
            if hits < 8
                || hits * 4 < count * 3
                || clear != hits
                || missing_side
                || folded != 0
                || inverted != 0
            {
                failures.push(format!("{label} {} front={front}: sampled={count}, support={hits}, visibly_clear={clear}, left/right={left}/{right}, overlapping_interiors={folded}, inverted_tops={inverted}, minimum_gap={:.6} at {:?}", band.name, worst.0, worst.1));
            }
        }
    }
    failures
}

#[test]
fn native_medusa_sewn_edges_remain_visible_over_the_emitted_body() {
    let bands = [
        // The armholes close under the arm: the bodice wraps the torso below.
        Band {
            name: "below the armholes",
            low: 0.12,
            high: 0.18,
            back: true,
            coverage: Coverage::Absent,
        },
        Band {
            name: "underarm armholes",
            low: 0.43,
            high: 0.46,
            back: true,
            coverage: Coverage::Bilateral,
        },
        Band {
            name: "upper armholes",
            low: 0.47,
            high: 0.5,
            back: true,
            coverage: Coverage::Bilateral,
        },
        Band {
            name: "lower neckline",
            low: 0.40,
            high: 0.445,
            back: false,
            coverage: Coverage::Bilateral,
        },
        Band {
            name: "upper neckline",
            low: 0.47,
            high: 0.52,
            back: false,
            coverage: Coverage::Bilateral,
        },
    ];
    let mut failures = Vec::new();
    for build in 0..3 {
        let mut look = female(CreatureModel::Medusa, build);
        look.trim = MARKER;
        assert!(look.outfit.is_none());
        let mut suppressed = look.clone();
        let mut outfit = Outfit::worn(&look);
        outfit.bodice.cut = Neckline::Closed;
        outfit.armhole_edge = None;
        // Its intentional metal neck band/disc is not a structural occluder.
        // Metal is excluded from the marked top faces, and removed in baseline.
        outfit.torso_ornament = None;
        suppressed.outfit = Some(outfit);
        failures.extend(check_visibility(
            &format!("native Medusa build {build}"),
            &look,
            &suppressed,
            MARKER,
            &bands,
        ));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn native_vampire_cravat_is_male_only_and_buttons_remain_for_both_sexes() {
    // The native cravat uses this literal pigment, not the shared CREAM.
    // Deliberately ignore material: moving a white tie to a different finish
    // must not make the female-style regression pass.
    let cravat = linear(Vec3::new(0.88, 0.85, 0.8));
    let button = linear(Vec3::new(0.9, 0.75, 0.35));
    let has_color =
        |tri: &Triangle, color: Vec3| tri.colors.iter().all(|c| c.distance(color) < 1e-5);
    let mut failures = Vec::new();
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Vampire,
                42,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            );
            assert!(look.outfit.is_none(), "Vampire fixture must remain native");
            let triangles = torso(&look);
            for (low, high) in [(0.28, 0.32), (0.35, 0.39), (0.42, 0.45)] {
                let mut faces = 0;
                let mut area = 0.0;
                for tri in &triangles {
                    let p = tri.center();
                    if !has_color(tri, cravat) || p.y < low || p.y > high || p.x.abs() > 0.06 {
                        continue;
                    }
                    let n = (tri.p[1] - tri.p[0]).cross(tri.p[2] - tri.p[0]);
                    if n.z > 0.0 {
                        faces += 1;
                        area += n.z * 0.5;
                    }
                }
                if (woman && faces != 0) || (!woman && (faces == 0 || area < 0.0001)) {
                    failures.push(format!("native Vampire woman={woman} build={build}, chest {low}..{high}: cravat top_faces={faces}, projected_area={area:.7}"));
                }
            }
            // The female bodice keeps its authored two columns of gold buttons;
            // removing the whole native costume block is not a valid fix.
            for height in [0.13, 0.21, 0.29] {
                for side in [-1.0, 1.0] {
                    let count = triangles
                        .iter()
                        .filter(|tri| {
                            let p = tri.center();
                            has_color(tri, button)
                                && p.x * side > 0.0
                                && (p.y - height).abs() < 0.015
                        })
                        .count();
                    if count == 0 {
                        failures.push(format!("native Vampire woman={woman} build={build}: missing button at y={height}, side={side}"));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn native_orc_upper_harness_remains_visible_above_its_short_shell() {
    // The mid-chest red cowl tail intentionally overlays the native strap.
    // These exposed upper rows test the transition from shell to body instead.
    let bands = [
        Band {
            name: "shell shoulder",
            low: 0.42,
            high: 0.44,
            back: true,
            coverage: Coverage::SingleStrip,
        },
        Band {
            name: "above shell",
            low: 0.44,
            high: 0.47,
            back: true,
            coverage: Coverage::SingleStrip,
        },
    ];
    let mut failures = Vec::new();
    for build in 0..3 {
        let look = female(CreatureModel::Orc, build);
        assert!(look.outfit.is_none());
        let mut outfit = Outfit::worn(&look);
        let Some(TorsoOrnament::Harness { strap, .. }) = outfit.torso_ornament else {
            panic!("native Orc harness recipe")
        };
        let mut suppressed = look.clone();
        outfit.torso_ornament = None;
        suppressed.outfit = Some(outfit);
        failures.extend(check_visibility(
            &format!("native Orc build {build}"),
            &look,
            &suppressed,
            strap,
            &bands,
        ));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn native_orc_scarf_tail_remains_visible_over_body_and_shell() {
    let bands = [
        Band {
            name: "lower scarf tail",
            low: 0.27,
            high: 0.32,
            back: false,
            coverage: Coverage::SingleStrip,
        },
        Band {
            name: "upper scarf tail",
            low: 0.33,
            high: 0.38,
            back: false,
            coverage: Coverage::SingleStrip,
        },
    ];
    let mut failures = Vec::new();
    for build in 0..3 {
        let mut look = female(CreatureModel::Orc, build);
        look.trim = MARKER;
        assert!(look.outfit.is_none());
        let mut outfit = Outfit::worn(&look);
        // Selecting the recipe suppresses the native cowl/tail. Remove the
        // harness too: the tail intentionally overlaps it, and this contract
        // measures structural body/shell contact, not decoration ordering.
        outfit.torso_ornament = None;
        let mut suppressed = look.clone();
        suppressed.outfit = Some(outfit);
        failures.extend(check_visibility(
            &format!("native Orc scarf build {build}"),
            &look,
            &suppressed,
            MARKER,
            &bands,
        ));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn transferred_orc_harness_remains_visible_over_human_body_and_shell() {
    let bands = [
        Band {
            name: "waist",
            low: 0.08,
            high: 0.18,
            back: true,
            coverage: Coverage::SingleStrip,
        },
        Band {
            name: "lower chest",
            low: 0.22,
            high: 0.29,
            back: true,
            coverage: Coverage::SingleStrip,
        },
        Band {
            name: "upper chest",
            low: 0.34,
            high: 0.41,
            back: true,
            coverage: Coverage::SingleStrip,
        },
        Band {
            name: "shell transition",
            low: 0.42,
            high: 0.47,
            back: true,
            coverage: Coverage::SingleStrip,
        },
    ];
    let mut failures = Vec::new();
    for build in 0..3 {
        let recipe = Outfit::worn(&female(CreatureModel::Orc, build));
        let mut look = female(CreatureModel::Human, build).dressed(Wardrobe::Commoner);
        let outfit = look.outfit.as_mut().unwrap();
        outfit.outer = recipe.outer;
        let Some(TorsoOrnament::Harness {
            ring,
            shoulder_straps,
            ..
        }) = recipe.torso_ornament
        else {
            panic!("transferable Orc harness recipe")
        };
        outfit.torso_ornament = Some(TorsoOrnament::Harness {
            strap: MARKER,
            ring,
            shoulder_straps,
        });
        let mut suppressed = look.clone();
        suppressed.outfit.as_mut().unwrap().torso_ornament = None;
        failures.extend(check_visibility(
            &format!("Human transferred Orc recipe build {build}"),
            &look,
            &suppressed,
            MARKER,
            &bands,
        ));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
