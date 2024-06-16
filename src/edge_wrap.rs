use bevy::{
    app::{App, First, Plugin, Update},
    asset::Handle,
    ecs::{
        component::Component,
        entity::Entity,
        query::{Changed, With},
        removal_detection::RemovedComponents,
        schedule::{common_conditions::resource_exists, IntoSystemConfigs, SystemSet},
        system::{Commands, Query, Res, ResMut, Resource},
    },
    gizmos::gizmos::Gizmos,
    hierarchy::DespawnRecursiveExt,
    log::debug,
    math::{Quat, Vec2, Vec3, Vec3Swizzles},
    render::color::Color,
    sprite::{ColorMaterial, MaterialMesh2dBundle, Mesh2dHandle},
    transform::components::{GlobalTransform, Transform},
    window::{PrimaryWindow, Window},
};
use bevy_rapier2d::{
    dynamics::RigidBody,
    geometry::{ActiveEvents, Collider, CollisionGroups},
    na::{Isometry2, Vector2},
};

pub struct EdgeWrapPlugin;

impl Plugin for EdgeWrapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Bounds>()
            .add_systems(First, sync_bounds_to_window_size)
            .add_systems(
                Update,
                draw_bounds_gizmos
                    .run_if(resource_exists::<BoundsDebug>)
                    .in_set(EdgeWrapSet),
            )
            .add_systems(
                Update,
                (
                    duplicable_removed,
                    duplicate_on_map_edge,
                    sync_duplicate_transforms,
                    teleport_original_to_swap,
                )
                    .chain()
                    .in_set(EdgeWrapSet),
            );
    }
}

#[derive(SystemSet, PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub struct EdgeWrapSet;

#[derive(Resource)]
pub struct BoundsDebug;

#[derive(Resource, Debug)]
pub struct Bounds(pub Vec2);

impl Default for Bounds {
    fn default() -> Self {
        Self(Vec2::new(500.0, 500.0))
    }
}

fn draw_bounds_gizmos(mut gizmos: Gizmos, bounds: Res<Bounds>) {
    gizmos.rect_2d(
        Vec2::ZERO,
        0.,
        Vec2::new(bounds.0.x * 2., bounds.0.y * 2.),
        Color::WHITE,
    );
}

fn sync_bounds_to_window_size(
    mut bounds: ResMut<Bounds>,
    window_query: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = window_query.get_single() else {
        *bounds = Bounds::default();
        return;
    };

    bounds.0 = Vec2::new(
        window.resolution.width() / 2.,
        window.resolution.height() / 2.,
    );
}

#[derive(Component)]
pub struct Duplicable;

#[derive(Component, Debug, Clone)]
struct Original {
    duplicate_x: Option<Entity>,
    duplicate_y: Option<Entity>,
    duplicate_xy: Option<Entity>,
}

#[derive(Component, Debug)]
pub struct Duplicate {
    pub original: Entity,
}

fn duplicate_on_map_edge(
    mut commands: Commands,
    mut duplicable_query: Query<
        (
            Entity,
            &GlobalTransform,
            &Collider,
            &mut Mesh2dHandle,
            &mut Handle<ColorMaterial>,
            Option<&mut Original>,
            Option<&CollisionGroups>,
        ),
        (With<Duplicable>, Changed<GlobalTransform>),
    >,
    bounds: Res<Bounds>,
) {
    for (
        entity,
        transform,
        collider,
        mesh_handle,
        material_handle,
        mut opt_original,
        opt_coll_groups,
    ) in duplicable_query.iter_mut()
    {
        let mut spawn_duplicates = |original: &mut Original| {
            if original.duplicate_x.is_some()
                && original.duplicate_y.is_some()
                && original.duplicate_xy.is_some()
            {
                return;
            }

            let positions = edge_positions(transform, collider, &bounds);

            let intersects_y = positions.top == Position::Intersecting
                || positions.bottom == Position::Intersecting;
            let intersects_x = positions.left == Position::Intersecting
                || positions.right == Position::Intersecting;

            if intersects_y && original.duplicate_y.is_none() {
                let offset_y = bounds.0.y * 2. - transform.translation().y.signum();
                let duplicate_y = spawn_duplicate(
                    &mut commands,
                    transform,
                    &mesh_handle,
                    &material_handle,
                    collider,
                    Vec3::new(0.0, offset_y, 0.0),
                    entity,
                    opt_coll_groups.cloned(),
                );
                original.duplicate_y = Some(duplicate_y);
                debug!("Spawning duplicate y for entity {:?}", entity);
            }

            if intersects_x && original.duplicate_x.is_none() {
                let offset_x = bounds.0.x * 2. * -transform.translation().x.signum();
                let duplicate_x = spawn_duplicate(
                    &mut commands,
                    transform,
                    &mesh_handle,
                    &material_handle,
                    collider,
                    Vec3::new(offset_x, 0.0, 0.0),
                    entity,
                    opt_coll_groups.cloned(),
                );
                original.duplicate_x = Some(duplicate_x);
                debug!("Spawning duplicate x for entity {:?}", entity);
            }

            if intersects_y && intersects_x && original.duplicate_xy.is_none() {
                let offset_xy = Vec2::new(
                    bounds.0.x * 2. * -transform.translation().x.signum(),
                    bounds.0.y * 2. * -transform.translation().y.signum(),
                );
                let duplicate_xy = spawn_duplicate(
                    &mut commands,
                    transform,
                    &mesh_handle,
                    &material_handle,
                    collider,
                    Vec3::new(offset_xy.x, offset_xy.y, 0.0),
                    entity,
                    opt_coll_groups.cloned(),
                );
                original.duplicate_xy = Some(duplicate_xy);
                debug!("Spawning duplicate xy for entity {:?}", entity);
            }
        };

        if let Some(original) = opt_original.as_mut() {
            spawn_duplicates(original);
        } else {
            let mut original = Original {
                duplicate_x: None,
                duplicate_y: None,
                duplicate_xy: None,
            };

            spawn_duplicates(&mut original);

            commands.entity(entity).insert(original);
        };
    }
}

fn spawn_duplicate(
    commands: &mut Commands,
    transform: &GlobalTransform,
    mesh_handle: &Mesh2dHandle,
    material_handle: &Handle<ColorMaterial>,
    collider: &Collider,
    offset: Vec3,
    original: Entity,
    opt_coll_groups: Option<CollisionGroups>,
) -> Entity {
    let mut spawn = commands.spawn((
        Duplicate { original },
        MaterialMesh2dBundle {
            mesh: mesh_handle.clone(),
            material: material_handle.clone(),
            transform: Transform::from_translation(transform.translation() + offset),
            ..Default::default()
        },
        collider.clone(),
        ActiveEvents::COLLISION_EVENTS,
        RigidBody::KinematicPositionBased,
    ));
    if let Some(coll_groups) = opt_coll_groups {
        spawn.insert(coll_groups);
    }
    spawn.id()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
    Inside,
    Intersecting,
    Outside,
}

struct EdgePositions {
    top: Position,
    bottom: Position,
    left: Position,
    right: Position,
}

fn edge_positions(
    global_transform: &GlobalTransform,
    collider: &Collider,
    bounds: &Bounds,
) -> EdgePositions {
    let (_, rot, pos) = global_transform.to_scale_rotation_translation();
    let pos = pos.xy();
    let rot = rot.angle_between(Quat::IDENTITY);

    let aabb = collider
        .as_trimesh()
        .unwrap()
        .raw
        .aabb(&Isometry2::new(Vector2::new(pos.x, pos.y), rot));

    let max_y = pos.y + aabb.half_extents().y;
    let min_y = pos.y - aabb.half_extents().y;
    let max_x = pos.x + aabb.half_extents().x;
    let min_x = pos.x - aabb.half_extents().x;

    let determine_position = |min_edge, max_edge, bound| {
        if min_edge > bound {
            Position::Outside
        } else if max_edge > bound {
            Position::Intersecting
        } else {
            Position::Inside
        }
    };

    let top = determine_position(min_y, max_y, bounds.0.y);
    let bottom = determine_position(-max_y, -min_y, bounds.0.y);
    let left = determine_position(-max_x, -min_x, bounds.0.x);
    let right = determine_position(min_x, max_x, bounds.0.x);

    EdgePositions {
        top,
        bottom,
        left,
        right,
    }
}

fn sync_duplicate_transforms(
    duplicable_query: Query<(&GlobalTransform, &Original), Changed<GlobalTransform>>,
    mut transform_query: Query<&mut Transform>,
    bounds: Res<Bounds>,
) {
    for (original_global_transform, original) in &duplicable_query {
        let original_pos = original_global_transform.translation();

        let mut update_duplicate_transform = |entity: Entity, duplicate_offset: Vec2| {
            let mut duplicate_transform = transform_query.get_mut(entity).unwrap();

            duplicate_transform.translation =
                original_pos + Vec3::new(duplicate_offset.x, duplicate_offset.y, 0.);

            duplicate_transform.rotation =
                original_global_transform.to_scale_rotation_translation().1;
        };

        if let Some(duplicate) = original.duplicate_x {
            update_duplicate_transform(
                duplicate,
                Vec2::new(bounds.0.x * 2. * -original_pos.x.signum(), 0.),
            );
        }

        if let Some(duplicate) = original.duplicate_y {
            update_duplicate_transform(
                duplicate,
                Vec2::new(0., bounds.0.y * 2. * -original_pos.y.signum()),
            );
        }

        if let Some(duplicate) = original.duplicate_xy {
            update_duplicate_transform(
                duplicate,
                Vec2::new(
                    bounds.0.x * 2. * -original_pos.x.signum(),
                    bounds.0.y * 2. * -original_pos.y.signum(),
                ),
            );
        }
    }
}

fn teleport_original_to_swap(
    mut commands: Commands,
    mut original_query: Query<
        (
            Entity,
            &GlobalTransform,
            &mut Transform,
            &Collider,
            &Original,
        ),
        Changed<GlobalTransform>,
    >,
    bounds: Res<Bounds>,
) {
    for (original_entity, global_transform, mut transform, collider, original) in
        &mut original_query
    {
        let mut remove_original_and_duplicates = || {
            commands.entity(original_entity).remove::<Original>();
            if let Some(duplicate) = original.duplicate_x {
                commands.entity(duplicate).despawn_recursive();
            }
            if let Some(duplicate) = original.duplicate_y {
                commands.entity(duplicate).despawn_recursive();
            }
            if let Some(duplicate) = original.duplicate_xy {
                commands.entity(duplicate).despawn_recursive();
            }

            debug!("Removing duplicates of entity {:?}", original_entity);
        };

        let edge_positions = edge_positions(global_transform, collider, &bounds);

        // Delete duplicates if the original is inside the bounds
        if edge_positions.top == Position::Inside
            && edge_positions.bottom == Position::Inside
            && edge_positions.left == Position::Inside
            && edge_positions.right == Position::Inside
        {
            remove_original_and_duplicates();
        }

        let original_pos = global_transform.translation().xy();

        let mut teleport_to_duplicate = |offset: Vec2| {
            transform.translation += Vec3::new(offset.x, offset.y, 0.0);

            debug!(
                "Teleporting entity {:?} to {:?}",
                original_entity, transform.translation
            );

            remove_original_and_duplicates();
        };

        // Teleport the original to the duplicate on the opposite side if it's outside the bounds
        if (edge_positions.top == Position::Outside || edge_positions.bottom == Position::Outside)
            && (edge_positions.left == Position::Outside
                || edge_positions.right == Position::Outside)
        {
            let offset = Vec2::new(
                bounds.0.x * 2. * -original_pos.x.signum(),
                bounds.0.y * 2. * -original_pos.y.signum(),
            );

            teleport_to_duplicate(offset);
        }

        if (edge_positions.top == Position::Outside || edge_positions.bottom == Position::Outside)
            && edge_positions.left == Position::Inside
            && edge_positions.right == Position::Inside
        {
            let offset = Vec2::new(0., bounds.0.y * 2. * -original_pos.y.signum());

            teleport_to_duplicate(offset);
        }

        if (edge_positions.left == Position::Outside || edge_positions.right == Position::Outside)
            && edge_positions.top == Position::Inside
            && edge_positions.bottom == Position::Inside
        {
            let offset = Vec2::new(bounds.0.x * 2. * -original_pos.x.signum(), 0.);

            teleport_to_duplicate(offset);
        }
    }
}

fn duplicable_removed(
    mut commands: Commands,
    mut removed: RemovedComponents<Duplicable>,
    duplicate_query: Query<(Entity, &Duplicate)>,
) {
    for entity in removed.read() {
        for (duplicate_entity, duplicate) in &duplicate_query {
            if duplicate.original == entity {
                debug!("Removing duplicate entity {:?}", duplicate_entity);
                commands.entity(duplicate_entity).despawn_recursive();
            }
        }
    }
}

pub fn get_original_entities(
    duplicate_query: &Query<&Duplicate, ()>,
    entity_a: &Entity,
) -> (Entity, Option<Entity>) {
    if let Ok(Duplicate { original }) = duplicate_query.get(*entity_a) {
        (*original, Some(*entity_a))
    } else {
        (*entity_a, None)
    }
}

#[cfg(test)]
mod tests {
    use bevy::{math::primitives::RegularPolygon, render::mesh::Mesh};

    use crate::utils::mesh_to_collider;

    use super::*;

    fn create_test_collider() -> Collider {
        let shape = RegularPolygon::new(10., 3);

        mesh_to_collider(&Mesh::from(shape)).expect("Failed to create collider")
    }

    fn create_test_transform(x: f32, y: f32, rotation: f32) -> GlobalTransform {
        GlobalTransform::IDENTITY.mul_transform(
            Transform::from_translation(Vec3::new(x, y, 0.0))
                .with_rotation(Quat::from_rotation_z(rotation)),
        )
    }

    fn create_test_bounds(distance: f32) -> Bounds {
        Bounds((distance, distance).into())
    }

    #[test]
    fn test_edge_positions_inside() {
        let bounds = create_test_bounds(500.0);
        let collider = create_test_collider();
        let transform = create_test_transform(0.0, 0.0, 0.0);

        let positions = edge_positions(&transform, &collider, &bounds);

        assert_eq!(positions.top, Position::Inside);
        assert_eq!(positions.bottom, Position::Inside);
        assert_eq!(positions.left, Position::Inside);
        assert_eq!(positions.right, Position::Inside);
    }

    #[test]
    fn test_edge_positions_outside() {
        let bounds = create_test_bounds(500.0);
        let collider = create_test_collider();
        let transform = create_test_transform(600.0, 600.0, 0.0);

        let positions = edge_positions(&transform, &collider, &bounds);

        assert_eq!(positions.top, Position::Outside);
        assert_eq!(positions.bottom, Position::Inside);
        assert_eq!(positions.left, Position::Inside);
        assert_eq!(positions.right, Position::Outside);
    }

    #[test]
    fn test_edge_positions_intersecting() {
        let bounds = create_test_bounds(500.0);
        let collider = create_test_collider();
        let transform = create_test_transform(500., 500., 0.);

        let positions = edge_positions(&transform, &collider, &bounds);

        assert_eq!(positions.top, Position::Intersecting);
        assert_eq!(positions.bottom, Position::Inside);
        assert_eq!(positions.left, Position::Inside);
        assert_eq!(positions.right, Position::Intersecting);
    }
}
