use bevy::{
    app::{App, Plugin, Update},
    asset::Handle,
    ecs::{
        component::Component,
        entity::Entity,
        query::{With, Without},
        schedule::{common_conditions::resource_exists, IntoSystemConfigs, SystemSet},
        system::{Commands, Query, Res, ResMut, Resource},
    },
    gizmos::gizmos::Gizmos,
    hierarchy::DespawnRecursiveExt,
    log::info,
    math::{Quat, Vec2, Vec3, Vec3Swizzles},
    render::color::Color,
    sprite::{ColorMaterial, MaterialMesh2dBundle, Mesh2dHandle},
    transform::components::{GlobalTransform, Transform},
    utils::default,
    window::Window,
};
use bevy_rapier2d::{
    geometry::Collider,
    na::{Isometry2, Vector2},
};

pub struct EdgeWrapPlugin;

impl Plugin for EdgeWrapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Bounds>().add_systems(
            Update,
            (
                sync_bounds_to_window_size,
                duplicate_on_map_edge,
                sync_duplicate_transforms,
                teleport_original_to_swap,
                draw_bounds_gizmos.run_if(resource_exists::<BoundsDebug>),
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

#[derive(Resource)]
pub struct Bounds(Vec2);

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

fn sync_bounds_to_window_size(mut bounds: ResMut<Bounds>, window_query: Query<&Window>) {
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

#[derive(Component)]
struct Original {
    duplicate: Entity,
}

#[derive(Component)]
struct Duplicate {
    original: Entity,
}

fn duplicate_on_map_edge(
    mut commands: Commands,
    duplicable_query: Query<
        (
            Entity,
            &GlobalTransform,
            &Collider,
            &mut Mesh2dHandle,
            &mut Handle<ColorMaterial>,
        ),
        (With<Duplicable>, Without<Original>),
    >,
    bounds: Res<Bounds>,
) {
    for (duplicable_entity, global_transform, collider, mesh_handle, material_handle) in
        &duplicable_query
    {
        let edge_positions = edge_positions(global_transform, collider, &bounds);

        let intersect_top = edge_positions.top == Position::Intersecting;
        let intersect_bottom = edge_positions.bottom == Position::Intersecting;
        let intersect_left = edge_positions.left == Position::Intersecting;
        let intersect_right = edge_positions.right == Position::Intersecting;

        if intersect_top || intersect_bottom || intersect_left || intersect_right {
            let duplicate_offset = duplicate_offset(global_transform, collider, &bounds);
            let duplicate_entity = commands
                .spawn((
                    Duplicate {
                        original: duplicable_entity,
                    },
                    MaterialMesh2dBundle {
                        mesh: mesh_handle.clone(),
                        material: material_handle.clone(),
                        transform: Transform::from_translation(Vec3::new(
                            global_transform.translation().x - duplicate_offset.x,
                            global_transform.translation().y - duplicate_offset.y,
                            0.,
                        )),
                        ..default()
                    },
                    collider.clone(),
                ))
                .id();

            commands.entity(duplicable_entity).insert(Original {
                duplicate: duplicate_entity,
            });

            info!(
                "Duplicating entity {:?} to {:?}",
                duplicable_entity, duplicate_entity
            );
        }
    }
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

    let top = if min_y > bounds.0.y {
        Position::Outside
    } else if max_y > bounds.0.y {
        Position::Intersecting
    } else {
        Position::Inside
    };

    let bottom = if max_y < -bounds.0.y {
        Position::Outside
    } else if min_y < -bounds.0.y {
        Position::Intersecting
    } else {
        Position::Inside
    };

    let left = if max_x < -bounds.0.x {
        Position::Outside
    } else if min_x < -bounds.0.x {
        Position::Intersecting
    } else {
        Position::Inside
    };

    let right = if min_x > bounds.0.x {
        Position::Outside
    } else if max_x > bounds.0.x {
        Position::Intersecting
    } else {
        Position::Inside
    };

    EdgePositions {
        top,
        bottom,
        left,
        right,
    }
}

fn duplicate_offset(
    global_transform: &GlobalTransform,
    collider: &Collider,
    bounds: &Bounds,
) -> Vec2 {
    let edge_positions = edge_positions(global_transform, collider, bounds);

    let pos = global_transform.translation().xy();

    let duplicate_offset_x =
        if edge_positions.left != Position::Inside || edge_positions.right != Position::Inside {
            bounds.0.x * 2. * -pos.x.signum()
        } else {
            0.
        };

    let duplicate_offset_y =
        if edge_positions.bottom != Position::Inside || edge_positions.top != Position::Inside {
            bounds.0.y * 2. * -pos.y.signum()
        } else {
            0.
        };

    (duplicate_offset_x, duplicate_offset_y).into()
}

fn sync_duplicate_transforms(
    duplicable_query: Query<(&GlobalTransform, &Collider, &Original)>,
    mut transform_query: Query<&mut Transform>,
    bounds: Res<Bounds>,
) {
    for (original_global_transform, collider, original) in &duplicable_query {
        let original_pos = original_global_transform.translation();
        let mut duplicate_transform = transform_query.get_mut(original.duplicate).unwrap();
        let duplicate_offset = duplicate_offset(original_global_transform, collider, &bounds);

        duplicate_transform.translation =
            original_pos + Vec3::new(duplicate_offset.x, duplicate_offset.y, 0.);

        duplicate_transform.rotation = original_global_transform.to_scale_rotation_translation().1;
    }
}

fn teleport_original_to_swap(
    mut commands: Commands,
    mut original_query: Query<(
        Entity,
        &GlobalTransform,
        &mut Transform,
        &Collider,
        &Original,
    )>,
    bounds: Res<Bounds>,
) {
    for (original_entity, global_transform, mut transform, collider, original) in
        &mut original_query
    {
        let edge_positions = edge_positions(global_transform, collider, &bounds);

        // Delete duplicates if the original is inside the bounds
        if edge_positions.top == Position::Inside
            && edge_positions.bottom == Position::Inside
            && edge_positions.left == Position::Inside
            && edge_positions.right == Position::Inside
        {
            commands.entity(original_entity).remove::<Original>();
            commands.entity(original.duplicate).despawn_recursive();
        }

        if edge_positions.top == Position::Outside
            || edge_positions.bottom == Position::Outside
            || edge_positions.left == Position::Outside
            || edge_positions.right == Position::Outside
        {
            let duplicate_offset = duplicate_offset(global_transform, collider, &bounds);

            let new_translation =
                transform.translation + Vec3::new(duplicate_offset.x, duplicate_offset.y, 0.);

            info!(
                "Teleporting original from {:?} to swap: {:?}",
                transform.translation, new_translation
            );

            transform.translation = new_translation;

            commands.entity(original_entity).remove::<Original>();
            commands.entity(original.duplicate).despawn_recursive();
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::{math::primitives::RegularPolygon, render::mesh::Mesh};

    use crate::utils::mesh_to_collider;

    use super::*;

    fn create_test_collider() -> Collider {
        let shape = RegularPolygon::new(10., 3);

        mesh_to_collider(&Mesh::from(shape))
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
        assert_eq!(positions.bottom, Position::Outside);
        assert_eq!(positions.left, Position::Outside);
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

    #[test]
    fn test_duplicate_offset_inside() {
        let bounds = create_test_bounds(500.0);
        let collider = create_test_collider();
        let transform = create_test_transform(0.0, 0.0, 0.0);

        let offset = duplicate_offset(&transform, &collider, &bounds);

        assert_eq!(offset, Vec2::ZERO);
    }

    #[test]
    fn test_duplicate_offset_outside() {
        let bounds = create_test_bounds(500.0);
        let collider = create_test_collider();
        let transform = create_test_transform(600.0, 600.0, 0.0);

        let offset = duplicate_offset(&transform, &collider, &bounds);

        assert_eq!(offset, Vec2::new(-1000.0, -1000.0));
    }

    #[test]
    fn test_duplicate_offset_intersecting() {
        let bounds = create_test_bounds(500.0);
        let collider = create_test_collider();
        let transform = create_test_transform(490.0, 490.0, 0.0);

        let offset = duplicate_offset(&transform, &collider, &bounds);

        assert_eq!(offset, Vec2::new(0.0, 0.0)); // Partially intersecting, so no full duplication
    }
}
