//! Connected, lined shoulder cloth. Recipes share construction and materials;
//! each wearer owns the vertices that follow its motion.
use super::{anatomy::HumanoidBody, *};
use bevy::asset::RenderAssetUsages;
use bevy::render::{
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    primitives::Aabb,
};

const ROWS: usize = 20;
const COLUMNS: usize = 48;
const GRID: usize = (ROWS + 1) * (COLUMNS + 1);
const THICKNESS: f32 = 0.012;

pub(super) struct Recipe {
    shoulders: f32,
    hem: f32,
    top: [Vec3; COLUMNS + 1],
    support: crate::sdf::Sculpt,
    pub length: f32,
    exterior: Vec3,
    lining: Vec3,
    ragged: bool,
}

impl Recipe {
    pub fn new(body: HumanoidBody, exterior: Vec3, lining: Vec3, ragged: bool) -> Self {
        let shoulders = 0.205 * body.shoulders * body.build * anatomy::HIP_SCALE.x;
        // The upper back has the same supports in the existing fitted torso
        // layers. Leather's ease keeps this yoke just above the cloth beneath.
        let support = geometry::torso_shape(body, Some(geometry::TorsoMaterial::Leather), false);
        let mut recipe = Self {
            shoulders,
            hem: 0.36 * body.hips_width * body.build * anatomy::HIP_SCALE.x,
            top: [Vec3::ZERO; COLUMNS + 1],
            support,
            length: 1.08,
            exterior,
            lining,
            ragged,
        };
        recipe.reserve_back_layer(0.0);
        recipe
    }

    /// Reserve the selected rear accessory layer before fitting the yoke and
    /// upper drape. A strap belongs underneath the cloak, not half through it.
    pub fn reserve_back_layer(&mut self, depth: f32) {
        // Reserve the lining too: fitting only the middle sheet to an
        // accessory envelope can leave its inner face cutting the strap.
        self.support
            .inflate(depth + if depth > 0.0 { THICKNESS * 0.5 } else { 0.0 });
        self.top = std::array::from_fn(|column| {
            let u = column as f32 / COLUMNS as f32 * 2.0 - 1.0;
            let x = u * self.shoulders / anatomy::HIP_SCALE.x;
            let y = 0.505 - 0.035 * u * u;
            let at = geometry::torso_surface_hit(&self.support, x, y, false)
                .expect("resolved shoulder yoke must have upper-back support");
            at * anatomy::HIP_SCALE + Vec3::NEG_Z * 0.012 - Vec3::new(0.0, 0.53, -0.15)
        });
    }

    fn point(&self, row: usize, column: usize) -> Vec3 {
        let t = row as f32 / ROWS as f32;
        let u = column as f32 / COLUMNS as f32 * 2.0 - 1.0;
        let spread = t * t * (3.0 - 2.0 * t);
        let width = self.shoulders + (self.hem - self.shoulders) * spread;
        // A gathered shoulder edge, broad hanging folds and a gently returned
        // side edge: the lining occupies the whole inside, not colored rods.
        let fold = (u * PI * 6.0).cos() * (0.006 + 0.027 * t);
        let returned_edge = u.abs().powi(4) * (0.09 - 0.035 * t);
        let hem = if self.ragged {
            // Connected notches of unequal depth, with a substantial web of
            // cloth above them. Never separate strips hanging in empty space.
            let teeth = (u * PI * 7.0 + 0.4).cos().abs();
            0.12 * teeth + 0.035 * (u * PI * 3.0).sin()
        } else {
            0.018 * (u * PI).cos()
        };
        let top = self.top[column];
        let mut point = Vec3::new(
            u * width,
            top.y * (1.0 - t) - self.length * t - hem * t.powi(8),
            top.z * (1.0 - t) + (-0.17 - 0.035 * (t * PI).sin() + returned_edge + fold) * t,
        );
        let at = (point + Vec3::new(0.0, 0.53, -0.15)) / anatomy::HIP_SCALE;
        if at.y >= 0.075 {
            if let Some(back) = geometry::torso_surface_hit(&self.support, at.x, at.y, false) {
                point.z = point.z.min(back.z * anatomy::HIP_SCALE.z - 0.012 + 0.15);
            }
        }
        point
    }
}

pub(super) struct CloakRig {
    pub entity: Entity,
    mesh: Handle<Mesh>,
    rest: Vec<Vec3>,
    points: Vec<Vec3>,
    normals: Vec<Vec3>,
    last_rotation: Quat,
}

fn index(row: usize, column: usize) -> usize {
    row * (COLUMNS + 1) + column
}

fn surface_normals(points: &[Vec3], normals: &mut [Vec3]) {
    for row in 0..=ROWS {
        for column in 0..=COLUMNS {
            let across = points[index(row, (column + 1).min(COLUMNS))]
                - points[index(row, column.saturating_sub(1))];
            let down = points[index((row + 1).min(ROWS), column)]
                - points[index(row.saturating_sub(1), column)];
            normals[index(row, column)] = across.cross(down).normalize_or(-Vec3::Z);
        }
    }
}

fn write(mesh: &mut Mesh, points: &[Vec3], normals: &[Vec3]) {
    if let Some(VertexAttributeValues::Float32x3(vertices)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for i in 0..GRID {
            vertices[i] = (points[i] + normals[i] * THICKNESS * 0.5).to_array();
            vertices[GRID + i] = (points[i] - normals[i] * THICKNESS * 0.5).to_array();
        }
    }
    if let Some(VertexAttributeValues::Float32x3(vertices)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    {
        for i in 0..GRID {
            vertices[i] = normals[i].to_array();
            vertices[GRID + i] = (-normals[i]).to_array();
        }
    }
}

pub(super) fn spawn(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    parent: Entity,
    recipe: Recipe,
    ghost: bool,
) -> CloakRig {
    let material = library
        .get(format!("creature/cloak-material/{ghost}"), meshes, || {
            let mut parts = Parts::default();
            parts.matte.tri([Vec3::ZERO, Vec3::X, Vec3::Y], Vec3::ONE);
            if ghost {
                parts.ghostify();
            }
            parts
        })
        .0[0]
        .1
        .clone();
    let rest: Vec<_> = (0..GRID)
        .map(|i| recipe.point(i / (COLUMNS + 1), i % (COLUMNS + 1)))
        .collect();
    let mut normals = vec![Vec3::ZERO; GRID];
    surface_normals(&rest, &mut normals);
    let mut indices = Vec::new();
    for row in 0..ROWS {
        for column in 0..COLUMNS {
            let a = index(row, column) as u32;
            let b = a + 1;
            let c = index(row + 1, column) as u32;
            let d = c + 1;
            indices.extend([a, b, c, b, d, c]);
            let g = GRID as u32;
            indices.extend([a + g, c + g, b + g, b + g, c + g, d + g]);
        }
    }
    // Follow the exterior boundary clockwise and bridge it to the lining.
    let perimeter: Vec<_> = (0..=COLUMNS)
        .map(|c| index(0, c))
        .chain((1..=ROWS).map(|r| index(r, COLUMNS)))
        .chain((0..COLUMNS).rev().map(|c| index(ROWS, c)))
        .chain((1..ROWS).rev().map(|r| index(r, 0)))
        .collect();
    for i in 0..perimeter.len() {
        let a = perimeter[i] as u32;
        let b = perimeter[(i + 1) % perimeter.len()] as u32;
        let g = GRID as u32;
        indices.extend([a, a + g, b, b, a + g, b + g]);
    }
    let pigment = |color: Vec3| {
        let display = color.min(Vec3::ONE);
        let linear = Color::srgb(display.x, display.y, display.z).to_linear();
        let excess = color - display;
        [
            linear.red + excess.x,
            linear.green + excess.y,
            linear.blue + excess.z,
            1.0,
        ]
    };
    let colors: Vec<_> = std::iter::repeat_n(pigment(recipe.exterior), GRID)
        .chain(std::iter::repeat_n(pigment(recipe.lining), GRID))
        .collect();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; GRID * 2])
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0; 3]; GRID * 2])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices));
    write(&mut mesh, &rest, &normals);
    let mesh = meshes.add(mesh);
    let entity = commands
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Transform::from_xyz(0.0, 0.53, -0.15),
            Visibility::default(),
            Aabb {
                center: bevy::math::Vec3A::new(0.0, -recipe.length * 0.5, -0.12),
                half_extents: bevy::math::Vec3A::new(1.0, 1.5, 1.0),
            },
        ))
        .id();
    commands.entity(parent).add_child(entity);
    CloakRig {
        entity,
        mesh,
        points: rest.clone(),
        rest,
        normals,
        last_rotation: Quat::IDENTITY,
    }
}

pub(super) fn place(cloak: &mut CloakRig, rotation: Quat, meshes: &mut Assets<Mesh>) {
    if rotation.angle_between(cloak.last_rotation) < 1e-6 {
        return;
    }
    // The drape entity retains its existing eased swing. Cancel that rotation
    // at the shoulder and introduce it gradually down the private rest grid.
    // Two fixed rows form a supported yoke, rather than a hinge-shaped gap.
    let inverse = rotation.inverse();
    for row in 0..=ROWS {
        let t = row.saturating_sub(1) as f32 / (ROWS - 1) as f32;
        let give = t * t * (3.0 - 2.0 * t);
        let bend = inverse * Quat::IDENTITY.slerp(rotation, give);
        for column in 0..=COLUMNS {
            let i = index(row, column);
            cloak.points[i] = bend * cloak.rest[i];
        }
    }
    surface_normals(&cloak.points, &mut cloak.normals);
    if let Some(mesh) = meshes.get_mut(&cloak.mesh) {
        write(mesh, &cloak.points, &cloak.normals);
    }
    cloak.last_rotation = rotation;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positions(mesh: &Mesh) -> Vec<Vec3> {
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("cloth positions")
        };
        points.iter().map(|p| Vec3::from_array(*p)).collect()
    }

    #[test]
    fn gathered_yoke_fits_the_resolved_shoulders_instead_of_floating_behind_them() {
        for model in [
            CreatureModel::Vampire,
            CreatureModel::Troll,
            CreatureModel::Zombie,
        ] {
            for woman in [false, true] {
                for build in 0..3 {
                    let look = Look::with_choice(
                        model,
                        42,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    );
                    let body = HumanoidBody::resolve(&look);
                    let recipe = Recipe::new(body, Vec3::ONE, Vec3::ZERO, false);
                    let support =
                        geometry::torso_shape(body, Some(geometry::TorsoMaterial::Leather), false);
                    let edge = recipe.point(0, COLUMNS);
                    let shoulder = body.shoulder(1.0).x;
                    assert!(
                        (0.85..1.05).contains(&(edge.x / shoulder)),
                        "{model:?} {woman} {build}: yoke must follow the wearer's shoulder span"
                    );
                    for column in 0..=COLUMNS {
                        let at = (recipe.point(0, column) + Vec3::new(0.0, 0.53, -0.15))
                            / anatomy::HIP_SCALE;
                        let gap = support.distance(at);
                        assert!((-0.005..0.03).contains(&gap), "{model:?} {woman} {build}: unsupported shoulder cloth at {at:?}, gap {gap}");
                    }
                }
            }
        }
    }

    #[test]
    fn shoulder_rows_stay_attached_and_motion_changes_only_the_wearers_mesh() {
        for ragged in [false, true] {
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
                    let body = HumanoidBody::resolve(&look);
                    let mut materials = Assets::<StandardMaterial>::default();
                    let mut library =
                        PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
                    let mut meshes = Assets::<Mesh>::default();
                    let mut world = World::new();
                    let parent = world.spawn_empty().id();
                    let mut queue = bevy::ecs::world::CommandQueue::default();
                    let mut commands = Commands::new(&mut queue, &world);
                    let recipe = || {
                        Recipe::new(
                            body,
                            Vec3::new(0.06, 0.04, 0.09),
                            Vec3::new(0.7, 0.05, 0.1),
                            ragged,
                        )
                    };
                    let mut first = spawn(
                        &mut commands,
                        &mut library,
                        &mut meshes,
                        parent,
                        recipe(),
                        false,
                    );
                    let second = spawn(
                        &mut commands,
                        &mut library,
                        &mut meshes,
                        parent,
                        recipe(),
                        false,
                    );
                    queue.apply(&mut world);
                    assert_ne!(first.mesh, second.mesh);
                    assert_eq!(
                        world
                            .get::<MeshMaterial3d<StandardMaterial>>(first.entity)
                            .unwrap()
                            .0,
                        world
                            .get::<MeshMaterial3d<StandardMaterial>>(second.entity)
                            .unwrap()
                            .0
                    );
                    let untouched = positions(meshes.get(&second.mesh).unwrap());
                    let colors = meshes
                        .get(&first.mesh)
                        .unwrap()
                        .attribute(Mesh::ATTRIBUTE_COLOR)
                        .unwrap()
                        .clone();
                    let indices = meshes.get(&first.mesh).unwrap().indices().unwrap().clone();
                    for rotation in [
                        Quat::from_rotation_x(0.35),
                        Quat::from_euler(EulerRot::XYZ, 0.3, 0.0, -0.3),
                        Quat::IDENTITY,
                    ] {
                        place(&mut first, rotation, &mut meshes);
                        let mesh = meshes.get(&first.mesh).unwrap();
                        let points = positions(mesh);
                        for i in 0..(COLUMNS + 1) * 2 {
                            let middle = rotation * (points[i] + points[GRID + i]) * 0.5;
                            assert!(
                                middle.distance(first.rest[i]) < 1e-5,
                                "shoulders must stay attached through travel and turns"
                            );
                        }
                        let hem = GRID - 1 - COLUMNS / 2;
                        let moved = rotation * (points[hem] + points[GRID + hem]) * 0.5;
                        if rotation != Quat::IDENTITY {
                            assert!(
                                moved.distance(first.rest[hem]) > 0.2,
                                "lower cloth must follow the eased swing"
                            );
                        }
                        for i in 0..GRID {
                            assert!(
                                (points[i].distance(points[GRID + i]) - THICKNESS).abs() < 1e-5
                            );
                        }
                        let bounds = world.get::<Aabb>(first.entity).unwrap();
                        assert!(points.iter().all(|point| point.is_finite()
                            && (bevy::math::Vec3A::from(*point) - bounds.center)
                                .abs()
                                .cmple(bounds.half_extents)
                                .all()));
                        let Some(VertexAttributeValues::Float32x3(normals)) =
                            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
                        else {
                            panic!("cloth normals")
                        };
                        assert!(normals
                            .iter()
                            .all(|n| (Vec3::from_array(*n).length() - 1.0).abs() < 1e-5));
                        assert_eq!(
                            format!("{:?}", mesh.attribute(Mesh::ATTRIBUTE_COLOR).unwrap()),
                            format!("{colors:?}")
                        );
                        assert_eq!(
                            format!("{:?}", mesh.indices().unwrap()),
                            format!("{indices:?}")
                        );
                        assert_eq!(untouched, positions(meshes.get(&second.mesh).unwrap()));
                    }
                    let restored = positions(meshes.get(&first.mesh).unwrap());
                    assert!(restored
                        .iter()
                        .zip(&untouched)
                        .all(|(a, b)| a.distance(*b) < 1e-5));
                }
            }
        }
    }

    #[test]
    fn both_sides_have_distinct_pigments_and_outward_winding() {
        let recipe = Recipe::new(
            HumanoidBody::resolve(&Look::of(CreatureModel::Vampire, 42)),
            Vec3::new(0.05, 0.03, 0.08),
            Vec3::new(0.7, 0.04, 0.06),
            false,
        );
        let mut materials = Assets::<StandardMaterial>::default();
        let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
        let mut meshes = Assets::<Mesh>::default();
        let mut world = World::new();
        let parent = world.spawn_empty().id();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let rig = spawn(
            &mut Commands::new(&mut queue, &world),
            &mut library,
            &mut meshes,
            parent,
            recipe,
            false,
        );
        let mesh = meshes.get(&rig.mesh).unwrap();
        let points = positions(mesh);
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("cloth pigments")
        };
        assert_ne!(colors[0], colors[GRID]);
        assert!(colors[..GRID].iter().all(|c| *c == colors[0]));
        assert!(colors[GRID..].iter().all(|c| *c == colors[GRID]));
        let triangles: Vec<_> = mesh.indices().unwrap().iter().collect();
        for triangle in triangles.as_chunks::<3>().0.iter().take(ROWS * COLUMNS * 4) {
            let [a, b, c] = [
                points[triangle[0]],
                points[triangle[1]],
                points[triangle[2]],
            ];
            let normal = (b - a).cross(c - a);
            assert!(normal.length() > 1e-6, "no zero-area cloth faces");
            let side = if triangle[0] < GRID { -1.0 } else { 1.0 };
            assert!(
                normal.z * side > 0.0,
                "lining and exterior face opposite directions"
            );
        }
    }
}
