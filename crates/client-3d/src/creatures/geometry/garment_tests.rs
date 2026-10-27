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
/// suppressing decorative layers. An explicit Orc outfit adds a neck cut above
/// the short shell. Verify unchanged geometry within the measured range.
fn check_visibility(
    label: &str,
    look: &Look,
    suppressed: &Look,
    pigment: Vec3,
    bands: &[Band],
) -> Vec<String> {
    let visible = torso(look);
    let baseline = torso(suppressed);
    check_triangle_visibility(label, visible, baseline, pigment, bands)
}

fn check_triangle_visibility(
    label: &str,
    visible: Vec<Triangle>,
    baseline: Vec<Triangle>,
    pigment: Vec3,
    bands: &[Band],
) -> Vec<String> {
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
        // Keep the cut: closing it changes the mesh partition at the skin/
        // cloth boundary. Recolor only trim to identify all its faces (including
        // dark sides and the intentional metal neck ornament), then remove those
        // faces from the support mesh. No fitting/SDF helper defines this oracle.
        let visible = torso(&look);
        let mut recolored = look.clone();
        recolored.trim = Vec3::new(0.13, 0.61, 0.29);
        let comparison = torso(&recolored);
        assert_eq!(visible.len(), comparison.len(), "trim changed topology");
        let baseline: Vec<_> = visible
            .iter()
            .zip(comparison)
            .filter_map(|(original, changed)| {
                assert_eq!(original.p, changed.p, "trim changed vertex placement");
                assert_eq!(original.metal, changed.metal, "trim changed material");
                (original.colors == changed.colors).then_some(*original)
            })
            .collect();
        assert!(
            baseline.len() < visible.len(),
            "trim recoloring had no effect"
        );
        failures.extend(check_triangle_visibility(
            &format!("native Medusa build {build}"),
            visible,
            baseline,
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
        outfit.neck_drape = None;
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
fn transferred_collar_buttons_survive_and_equipment_hides_only_the_resolved_copy() {
    let button = linear(Vec3::new(0.9, 0.75, 0.35));
    let buttons = |look: &Look| {
        torso(look)
            .into_iter()
            .filter(|tri| {
                tri.metal
                    && tri.colors.iter().all(|c| c.distance(button) < 1e-5)
                    && (0.1..0.32).contains(&tri.center().y)
            })
            .count()
    };
    for woman in [false, true] {
        let native = Look::with_choice(
            CreatureModel::Vampire,
            42,
            Some(Variant {
                woman,
                build: 1,
                ..default()
            }),
        );
        let expected = buttons(&native);
        assert!(expected > 12, "fixture must contain the authored buttons");
        let recipe = Outfit::native(&native);
        let mut explicit = native.clone();
        explicit.outfit = Some(recipe);
        assert_eq!(
            buttons(&explicit),
            expected,
            "explicit recipe lost collar buttons"
        );
        let mut recipient = Look::with_choice(
            CreatureModel::Human,
            42,
            Some(Variant {
                woman,
                build: 1,
                ..default()
            }),
        );
        recipient.outfit = Some(recipe);
        assert_eq!(
            buttons(&recipient),
            expected,
            "recipient lost the outfit's buttons"
        );
        let mut equipment = super::super::outfit::Equipment::default();
        equipment.select("breastplate").unwrap();
        let equipped = recipient.clone().equipped(equipment);
        assert_eq!(buttons(&equipped), 0, "plate must suppress sewn decoration");
        assert_eq!(
            equipped.outfit,
            Some(recipe),
            "coverage must not mutate the base recipe"
        );
        assert_eq!(buttons(&equipped.equipped(Default::default())), expected);
    }
}

#[test]
fn native_shell_openings_survive_explicit_outfit_transfer() {
    let positions = |look: &Look| {
        let outfit = Outfit::worn(look);
        let body = HumanoidBody::resolve(look);
        let shell = outfit_shell(body, &outfit, true).expect("native outer shell");
        let Some(Outer::Shell { top, underbust, .. }) = outfit.outer else {
            unreachable!()
        };
        let top = shell_top(body, top, underbust);
        let mut parts = Parts::default();
        shell.mesh(
            &mut parts.satin,
            Vec3::new(-0.36, -0.03, -0.2) * body.build.max(1.0),
            Vec3::new(0.36, top + 0.03, 0.22) * body.build.max(1.0),
            0.0075,
            &|_, _| Vec3::ONE,
        );
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let materials = crate::props::PropMaterials::new(&mut materials);
        let model = parts.finish(&mut meshes, &materials);
        assert_eq!(model.0.len(), 1);
        let mesh = meshes.get(&model.0[0].0).unwrap();
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("shell positions")
        };
        points.clone()
    };
    for model in [
        CreatureModel::Vampire,
        CreatureModel::Wraith,
        CreatureModel::Leprechaun,
    ] {
        let native = female(model, 1);
        let expected = positions(&native);
        assert!(expected.len() > 100, "fixture must emit a real shell");
        let mut explicit = native.clone();
        explicit.outfit = Some(Outfit::native(&native));
        let actual = positions(&explicit);
        assert_eq!(
            actual.len(),
            expected.len(),
            "{model:?}: transferred shell topology changed"
        );
        assert!(
            actual.iter().zip(&expected).all(|(a, b)| a == b),
            "{model:?}: transferred shell vertices changed"
        );
    }
}

#[test]
fn native_scarf_survives_explicit_outfit_transfer() {
    let mut native = female(CreatureModel::Orc, 1);
    native.trim = MARKER;
    let marked_tail = |look: &Look| {
        torso(look)
            .into_iter()
            .filter(|triangle| {
                triangle.painted(linear(MARKER)) && (0.27..0.38).contains(&triangle.center().y)
            })
            .count()
    };
    let expected = marked_tail(&native);
    assert!(expected > 8, "native scarf must have visible tail faces");
    let mut transferred = native.clone();
    transferred.outfit = Some(Outfit::native(&native));
    assert_eq!(
        marked_tail(&transferred),
        expected,
        "explicit outfit lost its native scarf"
    );
    let mut human = female(CreatureModel::Human, 1);
    human.outfit = transferred.outfit;
    assert!(
        marked_tail(&human) > 8,
        "scarf must follow the outfit to another body"
    );
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
        // Suppress the scarf and harness explicitly: the tail intentionally
        // overlaps the harness, and this measures structural body/shell contact,
        // not decoration ordering.
        outfit.neck_drape = None;
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

#[test]
fn study_tabards_keep_their_costume_palette_when_the_wearer_changes() {
    use super::super::outfit::Wardrobe;
    for model in [CreatureModel::Human, CreatureModel::Sentinel] {
        for woman in [false, true] {
            for wardrobe in [Wardrobe::Paladin, Wardrobe::Valkyrie] {
                let mut wearer = Look::with_choice(model, 42, Some(Variant { woman, ..default() }));
                wearer.cloth = MARKER;
                let dressed = wearer.dressed(wardrobe);
                let triangles = torso(&dressed);
                assert!(!triangles.iter().any(|t| t.painted(linear(MARKER))), "{model:?} {woman} {wardrobe:?}: a study tabard must not inherit the wearer's native cloth");
                assert!(
                    triangles
                        .iter()
                        .filter(|t| t.painted(linear(Vec3::new(0.065, 0.16, 0.40))))
                        .count()
                        > 100,
                    "{model:?} {woman} {wardrobe:?}: the fitted blue tabard must remain present"
                );
            }
        }
    }
}

#[test]
fn stacked_skirts_keep_outer_cloth_outside_their_underlayers() {
    use super::super::outfit::Wardrobe;
    for model in [
        CreatureModel::Human,
        CreatureModel::Nymph,
        CreatureModel::Troll,
    ] {
        for wardrobe in [Wardrobe::Cleric, Wardrobe::Druid, Wardrobe::Necromancer] {
            for woman in [false, true] {
                let look = Look::with_choice(model, 42, Some(Variant { woman, ..default() }))
                    .dressed(wardrobe);
                let (world, rig, _, meshes) = super::super::rogue_tests::spawn_one(&look);
                let layers: Vec<Vec<Triangle>> = rig
                    .drapes
                    .iter()
                    .filter(|(_, kind)| matches!(kind, Drape::Skirt(_)))
                    .map(|(root, _)| {
                        let mut triangles = Vec::new();
                        for child in world.get::<Children>(*root).unwrap().iter() {
                            let Some(mesh) =
                                world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0))
                            else {
                                continue;
                            };
                            let Some(VertexAttributeValues::Float32x3(points)) =
                                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                            else {
                                panic!("positions")
                            };
                            let transform = world.get::<Transform>(child).unwrap().compute_affine();
                            let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                            triangles.extend(indices.as_chunks::<3>().0.iter().map(|index| {
                                Triangle {
                                    p: index
                                        .map(|i| transform.transform_point3(Vec3::from(points[i]))),
                                    colors: [Vec3::ZERO; 3],
                                    metal: false,
                                }
                            }));
                        }
                        triangles
                    })
                    .collect();
                assert!(layers.len() >= 2);
                let mut compared = 0;
                for pair in layers.windows(2) {
                    for side in 0..32 {
                        let angle = (side as f32 + 0.37) * TAU / 32.0;
                        let radial = Vec3::new(angle.sin(), 0.0, angle.cos());
                        let tangent = Vec3::new(angle.cos(), 0.0, -angle.sin());
                        let depths = |layer: &Vec<Triangle>, y: f32| {
                            layer
                                .iter()
                                .filter_map(|tri| {
                                    let mut rotated = *tri;
                                    rotated.p = tri
                                        .p
                                        .map(|p| Vec3::new(p.dot(tangent), p.y, p.dot(radial)));
                                    rotated.depth(Vec2::new(0.0, y)).filter(|d| *d > 0.0)
                                })
                                .reduce(f32::max)
                        };
                        for y in [-0.06, -0.10, -0.16, -0.22, -0.30] {
                            if let (Some(outer), Some(inner)) =
                                (depths(&pair[0], y), depths(&pair[1], y))
                            {
                                compared += 1;
                                assert!(outer>inner+0.001,"{wardrobe:?} woman={woman}: underlayer crosses outer cloth at angle={angle}, y={y}: outer={outer}, inner={inner}");
                            }
                        }
                    }
                }
                assert!(
                    compared > 80,
                    "must compare actual overlapping cloth panels, not only their openings"
                );
            }
        }
    }
}

#[test]
fn breastplate_clears_emitted_underlayers_and_cloak_clears_plate_through_swing() {
    for model in [CreatureModel::Human, CreatureModel::Orc] {
        for woman in [false, true] {
            for wardrobe in [Wardrobe::RoyalPrincess, Wardrobe::Ranger, Wardrobe::Knight] {
                let mut base = Look::with_choice(model, 17, Some(Variant { woman, ..default() }))
                    .dressed(wardrobe);
                // The cloak is an outer layer; it is tested separately below.
                base.outfit.as_mut().unwrap().cloak = None;
                let mut equipment = Equipment::default();
                equipment.select("breastplate").unwrap();
                let armored = base.clone().equipped(equipment);
                let armor: Vec<_> = torso(&armored).into_iter().filter(|t| t.metal).collect();
                let armor_support = Support::new(armor.clone());
                let opening = armor_parts::opening(HumanoidBody::resolve(&armored)).map(|shape| {
                    let mut cut = crate::sdf::Sculpt::default();
                    cut.add(shape, 0.0);
                    cut
                });
                let mut checked = 0;
                let mut underlayers = base.clone();
                underlayers.outfit.as_mut().unwrap().baldric = None;
                for triangle in torso(&underlayers).into_iter().filter(|t| !t.metal) {
                    let at = triangle.center();
                    if !(0.11..0.40).contains(&at.y) || at.x.abs() > 0.09 || at.z.abs() < 0.04 {
                        continue;
                    }
                    // Garment or skin visible through the intentional neckline
                    // is not an underlayer penetration of the breastplate.
                    if opening.as_ref().is_some_and(|cut| cut.distance(at) < 0.02) {
                        continue;
                    }
                    let front = at.z > 0.0;
                    let outside = armor_support
                        .depth(at, front)
                        .expect("breastplate covers core torso");
                    let gap = if front {
                        outside - at.z
                    } else {
                        at.z - outside
                    };
                    assert!(
                    gap > 0.004,
                    "{model:?} woman={woman} {wardrobe:?}: underlying torso pierces armor at {at:?}, gap={gap}"
                );
                    checked += 1;
                }
                assert!(
                    checked > 100,
                    "substantial emitted underlayer witness: {checked}"
                );
                equipment.select("cloak").unwrap();
                let look = base.equipped(equipment);
                let (world, mut rig, _, mut meshes) =
                    crate::creatures::rogue_tests::spawn_one(&look);
                assert_eq!(rig.cloaks.len(), 1);
                let cloak = &mut rig.cloaks[0];
                let entity = cloak.entity;
                for rotation in [
                    Quat::IDENTITY,
                    Quat::from_rotation_x(0.35),
                    Quat::from_rotation_x(-0.18),
                    Quat::from_euler(EulerRot::XYZ, 0.3, 0.0, -0.25),
                ] {
                    crate::creatures::cloak::place(cloak, rotation, &mut meshes);
                    let mesh = meshes.get(&world.get::<Mesh3d>(entity).unwrap().0).unwrap();
                    let Some(VertexAttributeValues::Float32x3(points)) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    else {
                        panic!("cloak positions")
                    };
                    let mut transform = *world.get::<Transform>(entity).unwrap();
                    transform.rotation = rotation;
                    let transform = transform.compute_affine();
                    let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                    let cloth = Support::new(
                        indices
                            .as_chunks::<3>()
                            .0
                            .iter()
                            .map(|tri| Triangle {
                                p: tri.map(|i| {
                                    transform.transform_point3(Vec3::from_array(points[i]))
                                }),
                                colors: [Vec3::ZERO; 3],
                                metal: false,
                            })
                            .collect(),
                    );
                    let mut checked = 0;
                    for triangle in &armor {
                        let at = triangle.center();
                        if at.z >= -0.04 || !(0.11..0.44).contains(&at.y) || at.x.abs() > 0.13 {
                            continue;
                        }
                        let inside = cloth
                            .depth(at, true)
                            .expect("cloak covers central back plate");
                        assert!(inside<at.z-0.002,"{model:?} woman={woman} {wardrobe:?} rotation={rotation:?}: plate pierces cloak at {at:?}, lining={inside}");
                        checked += 1;
                    }
                    assert!(checked > 100, "substantial rear armor witness: {checked}");
                }
            }
        }
    }
}

#[test]
fn equipped_plate_covers_lateral_torso_underlayers_in_sylvan_fixture() {
    let mut base = Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant {
            woman: true,
            bust_lean: 1,
            ..default()
        }),
    )
    .dressed(Wardrobe::SylvanDuelist);
    base.outfit.as_mut().unwrap().cloak = None;
    let mut equipment = Equipment::default();
    equipment.select("breastplate").unwrap();
    let armored = base.clone().equipped(equipment);
    let armor = Support::new(
        torso(&armored)
            .into_iter()
            .filter(|t| t.metal && (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).z > 0.0)
            .collect(),
    );
    let mut checked = 0;
    for triangle in torso(&base).into_iter().filter(|t| !t.metal) {
        let at = triangle.center();
        if !(0.10..0.20).contains(&at.x.abs()) || !(0.12..0.32).contains(&at.y) || at.z < 0.04 {
            continue;
        }
        let outside = armor
            .depth(at, true)
            .expect("lateral chest must be protected");
        assert!(
            outside - at.z > 0.004,
            "lateral underlayer at {at:?} breaches armor: outside={outside}, gap={}",
            outside - at.z
        );
        checked += 1;
    }
    assert!(
        checked > 100,
        "substantial lateral surface coverage: {checked}"
    );
}

#[test]
fn loose_front_locks_clear_equipped_torso_surface() {
    let mut equipment = Equipment::default();
    equipment.select("breastplate").unwrap();
    let look = Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant {
            woman: true,
            bust_lean: 1,
            ..default()
        }),
    )
    .dressed(Wardrobe::Leotard)
    .equipped(equipment);
    let support = Support::new(torso(&look).into_iter().filter(|t| t.metal).collect());
    let (world, rig, _, meshes) = crate::creatures::rogue_tests::spawn_one(&look);
    let hair = rig
        .drapes
        .iter()
        .find(|(_, kind)| *kind == Drape::Hair)
        .unwrap()
        .0;
    let head = rig.joint(J::Head).unwrap();
    let frame = world.get::<Transform>(head).unwrap().compute_affine()
        * world.get::<Transform>(hair).unwrap().compute_affine();
    let mut checked = 0;
    for child in world.get::<Children>(hair).unwrap().iter() {
        let mesh = meshes.get(&world.get::<Mesh3d>(child).unwrap().0).unwrap();
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("hair vertices")
        };
        let transform = frame * world.get::<Transform>(child).unwrap().compute_affine();
        for point in points {
            let at = transform.transform_point3(Vec3::from_array(*point));
            if !(0.12..0.40).contains(&at.y) || at.z < 0.0 {
                continue;
            }
            let Some(outside) = support.depth(at, true) else {
                continue;
            };
            assert!(
                at.z > outside + 0.003,
                "front hair penetrates armor at {at:?}, armor={outside}"
            );
            checked += 1;
        }
    }
    assert!(checked > 20, "front hair must be checked, count={checked}");
}

#[test]
fn every_rogue_body_armour_builds_its_own_torso_on_both_sexes() {
    use super::super::outfit::BodyItem;
    for woman in [false, true] {
        let base = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman,
                bust: 3,
                ..default()
            }),
        )
        .dressed(Wardrobe::Commoner);
        let bare = torso(&base);
        let mut seen = std::collections::HashSet::new();
        for (name, item) in BodyItem::NAMES {
            let mut equipment = Equipment::default();
            equipment.select(name).unwrap();
            assert_eq!(equipment.body, Some(item));
            let triangles = torso(&base.clone().equipped(equipment));
            let metal = triangles.iter().filter(|t| t.metal).count();
            let matte = triangles.len() - metal;
            assert!(
                triangles.len() > bare.len(),
                "{name} (woman={woman}) adds armour to the torso"
            );
            // Leather armours are leather; the rest are mostly metal.
            let leathern = matches!(
                item,
                BodyItem::Leather
                    | BodyItem::StuddedLeather
                    | BodyItem::RingMail
                    | BodyItem::SplintMail
            );
            let bare_metal = bare.iter().filter(|t| t.metal).count();
            if item == BodyItem::Leather {
                assert!(
                    metal - bare_metal < 200,
                    "leather is not metal-shelled: {} metal triangles",
                    metal - bare_metal
                );
            } else if !leathern {
                assert!(metal - bare_metal > 1000, "{name} has a metal surface");
            }
            let armoured = base.clone().equipped(equipment);
            let limbs = super::super::outfit::Composition::resolve(&armoured).outfit;
            if item == BodyItem::PlateMail {
                // Plate mail shares the breastplate's torso and arms the limbs.
                assert!(matches!(limbs.arms, Arms::Plate) && matches!(limbs.legs, Legs::Plate));
                continue;
            }
            assert_eq!(
                matches!(limbs.arms, Arms::Plate),
                matches!(item, BodyItem::SplintMail | BodyItem::BandedMail),
                "{name}: only splint and banded armour arm the limbs here"
            );
            assert!(
                seen.insert((metal, matte)),
                "{name} (woman={woman}) is indistinguishable from another armour"
            );
        }
    }
}
