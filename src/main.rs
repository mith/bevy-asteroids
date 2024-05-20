mod asteroids;
mod edge_wrap;
mod input;
mod mesh_utils;
mod player;
mod ship;
mod split_mesh;
mod turret;
mod utils;

use asteroids::{
    despawn_asteroids, spawn_asteroids, Asteroid, ASTEROID_MAX_SPAWN_ANG_VELOCITY,
    ASTEROID_MAX_SPAWN_LIN_VELOCITY, FROZEN_ASTEROIDS,
};
use bevy::{
    prelude::*,
    sprite::{MaterialMesh2dBundle, Mesh2dHandle},
};
use bevy_rapier2d::{
    dynamics::{RigidBody, Sleeping, Velocity},
    geometry::{ActiveEvents, Restitution},
    na::ComplexField,
    prelude::{CollisionEvent, NoUserData, RapierConfiguration, RapierPhysicsPlugin},
    render::RapierDebugRenderPlugin,
};
use edge_wrap::{Duplicable, EdgeWrapPlugin, EdgeWrapSet};
use input::{PlayerInputPlugin, PlayerInputSet};
use itertools::Itertools;
use mesh_utils::{calculate_mesh_area, distance_to_plane, get_intersection_points_2d};
use player::{despawn_player, spawn_player, Player};
use rand::{rngs::ThreadRng, Rng};
use ship::ship_movement;
use split_mesh::split_mesh;
use turret::{fire_projectile, reload, FireEvent, Projectile};
use utils::mesh_to_collider;

const PHYSICS_LENGTH_UNIT: f32 = 100.0;

fn main() {
    let mut app = App::new();

    let mut rapier_configuration = RapierConfiguration::new(PHYSICS_LENGTH_UNIT);
    rapier_configuration.gravity = Vec2::new(0., 0.);

    app.insert_resource(ClearColor(Color::BLACK))
        .add_plugins(DefaultPlugins)
        .insert_resource(rapier_configuration)
        .add_plugins((
            RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(PHYSICS_LENGTH_UNIT),
            // RapierDebugRenderPlugin::default(),
        ))
        .init_state::<GameState>()
        .add_plugins((EdgeWrapPlugin, PlayerInputPlugin))
        .add_systems(Startup, setup_camera)
        .add_systems(OnEnter(GameState::Playing), spawn_player)
        .add_systems(OnEnter(GameState::Playing), spawn_asteroids)
        .add_systems(
            OnExit(GameState::Finished),
            (despawn_player, despawn_asteroids),
        )
        .add_systems(OnEnter(GameState::Finished), show_game_finished)
        .add_systems(OnExit(GameState::Finished), clear_game_result)
        .configure_sets(Update, (PlayerInputSet, EdgeWrapSet).chain())
        .add_event::<FireEvent>()
        .add_systems(
            Update,
            (
                reload,
                apply_deferred,
                fire_projectile,
                projectile_asteroid_collision,
                apply_deferred,
                split_long_skinny_asteroids,
            )
                .chain()
                .after(PlayerInputSet),
        )
        .add_systems(
            Update,
            (ship_movement, apply_deferred, player_asteroid_collision)
                .chain()
                .after(EdgeWrapSet),
        );

    app.run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Default, States)]
enum GameState {
    #[default]
    Playing,
    Finished,
}

fn player_asteroid_collision(
    mut collision_events: EventReader<CollisionEvent>,
    player_query: Query<Entity, With<Player>>,
    mut next_gamestate: ResMut<NextState<GameState>>,
) {
    for event in collision_events.read() {
        if let CollisionEvent::Started(entity_a, entity_b, _) = event {
            let player_entity = player_query.single();
            if player_entity == *entity_a || player_entity == *entity_b {
                info!("Player collided with asteroid");
                next_gamestate.set(GameState::Finished);
            }
        }
    }
}

fn split_asteroid(
    commands: &mut Commands,
    original_mesh: &Handle<Mesh>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    transform: &Transform,
    collision_direction: Vec2,
    rng: &mut ThreadRng,
) {
    let mesh = meshes.get(original_mesh).expect("Original mesh not found");

    // Rotate the collision direction by the rotation of the asteroid
    // to get the collision direction in the asteroid's local space.
    let asteroid_rotation = transform.rotation;
    let mesh_collision_direction = asteroid_rotation
        .inverse()
        .mul_vec3(collision_direction.extend(0.))
        .truncate()
        .normalize();

    let [(mesh_a, local_offset_a), (mesh_b, local_offset_b)] =
        split_mesh(mesh, mesh_collision_direction);

    let [offset_a, offset_b] = [local_offset_a, local_offset_b];

    let min_area = 500.;

    // Calculate area of the split mesh
    // Skip spawning if the area of the split mesh is too small

    if calculate_mesh_area(&mesh_a) > min_area {
        let translation = transform.transform_point(offset_a.extend(0.));
        let transform_a =
            Transform::from_translation(translation).with_rotation(transform.rotation);

        // Spawn new asteroid entities
        spawn_asteroid_split(rng, commands, transform_a, meshes, materials, &mesh_a);
    }

    if calculate_mesh_area(&mesh_b) > min_area {
        let translation = transform.transform_point(offset_b.extend(0.));
        let transform_b =
            Transform::from_translation(translation).with_rotation(transform.rotation);

        spawn_asteroid_split(rng, commands, transform_b, meshes, materials, &mesh_b);
    }
}

fn split_long_skinny_asteroids(
    mut commands: Commands,
    asteroid_query: Query<(Entity, &Transform, &mut Mesh2dHandle), With<Asteroid>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // Split asteroids if their aspect ratio is too high
    for (asteroid_entity, transform, mesh_handle) in &mut asteroid_query.iter() {
        let mesh = meshes.get(&mesh_handle.0).unwrap();
        let (normal, aspect_ratio) = mesh_aspect_ratio(mesh);

        if aspect_ratio > 6.0 {
            split_asteroid(
                &mut commands,
                &mesh_handle.0,
                &mut meshes,
                &mut materials,
                transform,
                normal,
                &mut rand::thread_rng(),
            );
            commands.entity(asteroid_entity).despawn();
        }
    }
}

fn mesh_aspect_ratio(mesh: &Mesh) -> (Vec2, f32) {
    let vertices = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .unwrap()
        .as_float3()
        .unwrap();
    // Find the longest line segment through the center of the asteroid
    // Assume the asteroid is convex
    let mut max_length = 0.;
    let mut max_length_indices = (0, 0);
    for (i, vertex) in vertices.iter().enumerate() {
        let vertex = Vec2::new(vertex[0], vertex[1]);
        for (j, other_vertex) in vertices[i + 1..].iter().enumerate() {
            let other_vertex = Vec2::new(other_vertex[0], other_vertex[1]);

            let length = (vertex - other_vertex).length();
            if length > max_length {
                max_length = length;
                max_length_indices = (i, i + j + 1);
            }
        }
    }

    let indices = mesh.indices().unwrap();

    let (i, j) = max_length_indices;
    let vertex_a = Vec2::new(vertices[i][0], vertices[i][1]);
    let vertex_b = Vec2::new(vertices[j][0], vertices[j][1]);
    let direction = (vertex_b - vertex_a).normalize();

    let normal = Vec2::new(-direction.y, direction.x);

    let normal_plane = Plane2d::new(*Direction2d::new(direction).unwrap());

    let mesh_center = vertices
        .iter()
        .fold(Vec2::ZERO, |acc, v| acc + Vec2::new(v[0], v[1]))
        / vertices.len() as f32;

    // Calculate the max distance to the longest line segment
    // For every edge of the asteroid, calculate the projection of the edge onto the normal
    // and find the maximum projection.
    let mut width = 0.0;
    for chunk in &indices.iter().chunks(3) {
        let mut side_a = Vec::new();
        let mut side_b = Vec::new();

        for index in chunk {
            let vertex = Vec2::new(vertices[index][0], vertices[index][1]);
            if distance_to_plane(vertex, normal_plane, Vec2::ZERO) > 0.0 {
                side_a.push(index);
            } else {
                side_b.push(index);
            }
        }

        match (side_a.len(), side_b.len()) {
            (1, 2) => {
                let intersections = get_intersection_points_2d(
                    &normal_plane,
                    vertices,
                    side_a[0],
                    &side_b,
                    mesh_center,
                );

                let distance = (intersections[0] - intersections[1]).length().abs();
                width += distance;
            }
            (2, 1) => {
                let intersections = get_intersection_points_2d(
                    &normal_plane,
                    vertices,
                    side_b[0],
                    &side_a,
                    mesh_center,
                );

                let distance = (intersections[0] - intersections[1]).length().abs();
                width += distance;
            }
            _ => {}
        }
    }

    let aspect_ratio = max_length / width;
    (normal, aspect_ratio)
}

fn spawn_asteroid_split(
    rng: &mut ThreadRng,
    commands: &mut Commands,
    transform: Transform,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    mesh: &Mesh,
) {
    let asteroid_velocity = Vec2::new(
        rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY),
        rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY),
    );
    let asteroid_angular_velocity =
        rng.gen_range(-ASTEROID_MAX_SPAWN_ANG_VELOCITY..ASTEROID_MAX_SPAWN_ANG_VELOCITY);

    let collider = mesh_to_collider(mesh);

    let mut asteroid_cmd = commands.spawn((
        Asteroid,
        MaterialMesh2dBundle {
            transform,
            mesh: Mesh2dHandle(meshes.add(mesh.clone())),
            material: materials.add(ColorMaterial::from(Color::WHITE)),
            ..default()
        },
        collider,
        ActiveEvents::COLLISION_EVENTS,
        Duplicable,
    ));
    if !FROZEN_ASTEROIDS {
        asteroid_cmd.insert((
            RigidBody::Dynamic,
            Velocity {
                linvel: asteroid_velocity,
                angvel: asteroid_angular_velocity,
            },
            Restitution {
                coefficient: 0.9,
                ..default()
            },
            Sleeping {
                normalized_linear_threshold: 0.001,
                angular_threshold: 0.001,
                ..default()
            },
        ));
    } else {
        asteroid_cmd.insert(RigidBody::Fixed);
    }
}

fn projectile_asteroid_collision(
    mut commands: Commands,
    mut collision_events: EventReader<CollisionEvent>,
    projectile_query: Query<&Projectile>,
    mut asteroid_query: Query<(&Transform, &mut Mesh2dHandle), With<Asteroid>>,
    transform_query: Query<&Transform>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mut rng = rand::thread_rng();
    for event in collision_events.read() {
        if let CollisionEvent::Started(entity_a, entity_b, _) = event {
            let (projectile_entity, asteroid_entity) = if projectile_query.contains(*entity_a)
                && asteroid_query.contains(*entity_b)
            {
                (*entity_a, *entity_b)
            } else if projectile_query.contains(*entity_b) && asteroid_query.contains(*entity_a) {
                (*entity_b, *entity_a)
            } else {
                continue;
            };

            let collision_direction = transform_query.get(projectile_entity).unwrap().translation
                - transform_query.get(asteroid_entity).unwrap().translation;

            commands.entity(projectile_entity).despawn();
            // Split asteroid into smaller asteroids
            if let Ok((transform, mesh_handle)) = asteroid_query.get_mut(asteroid_entity) {
                split_asteroid(
                    &mut commands,
                    &mesh_handle.0,
                    &mut meshes,
                    &mut materials,
                    transform,
                    collision_direction.xy().normalize(),
                    &mut rng,
                );
                commands.entity(asteroid_entity).despawn();
            }
        }
    }
}

#[derive(Component)]
struct FinishedText;

const FONT_PATH: &str = "fonts/PublicPixel-z84yD.ttf";

fn show_game_finished(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            FinishedText,
            Name::new("Finished screen"),
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Name::new("Game result text"),
                TextBundle::from_section(
                    "Game over",
                    TextStyle {
                        font: asset_server.load(FONT_PATH),
                        font_size: 30.,
                        color: Color::WHITE,
                    },
                ),
            ));
        });
}

fn clear_game_result(mut commands: Commands, finish_text_query: Query<Entity, With<FinishedText>>) {
    for finished_text_entity in &finish_text_query {
        commands.entity(finished_text_entity).despawn_recursive();
    }
}

#[cfg(test)]
mod tests {
    use bevy::render::{
        mesh::{Indices, PrimitiveTopology},
        render_asset::RenderAssetUsages,
    };

    use super::*;

    #[test]
    fn test_mesh_aspect_ratio_triangle() {
        let vertices = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 2.0, 0.0]];

        let indices = vec![0, 1, 2];

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
        mesh.insert_indices(Indices::U32(indices));

        let (normal, aspect_ratio) = mesh_aspect_ratio(&mesh);

        assert_eq!(normal, Vec2::new(-1., 0.));

        assert_eq!(aspect_ratio, 2.0);
    }

    #[test]
    fn test_split_asteroid_rectangle() {
        let mut app = App::new();

        app.insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<ColorMaterial>::default());

        app.add_systems(
            Startup,
            |mut commands: Commands,
             mut meshes: ResMut<Assets<Mesh>>,
             mut materials: ResMut<Assets<ColorMaterial>>| {
                let rectangle_shape =
                    bevy::math::primitives::Rectangle::from_size(Vec2::new(100., 100.));
                let asteroid_mesh = Mesh::from(rectangle_shape);
                let mesh_handle = meshes.add(asteroid_mesh.clone());

                let transform = Transform::default();

                let mut rng = rand::thread_rng();

                split_asteroid(
                    &mut commands,
                    &mesh_handle,
                    &mut meshes,
                    &mut materials,
                    &transform,
                    Vec2::new(0., 1.),
                    &mut rng,
                );
            },
        );

        app.run();

        // Check that 2 splits were created
        // They should be located at (-25, 0) and (25, 0)

        assert_eq!(app.world.query::<&Asteroid>().iter(&app.world).len(), 2);

        app.world
            .query::<(&Transform, &Asteroid)>()
            .iter(&app.world)
            .for_each(|(transform, _)| {
                let translation = transform.translation;
                assert!(translation.x == -25. || translation.x == 25.);
                assert_eq!(translation.y, 0.);
            });
    }

    #[test]
    fn test_split_asteroid_rectangle_90_cw_rotated() {
        let mut app = App::new();

        app.insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<ColorMaterial>::default());

        app.add_systems(
            Startup,
            |mut commands: Commands,
             mut meshes: ResMut<Assets<Mesh>>,
             mut materials: ResMut<Assets<ColorMaterial>>| {
                let rectangle_shape =
                    bevy::math::primitives::Rectangle::from_size(Vec2::new(100., 100.));
                let asteroid_mesh = Mesh::from(rectangle_shape);
                let mesh_handle = meshes.add(asteroid_mesh.clone());

                let transform =
                    Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));

                let mut rng = rand::thread_rng();

                split_asteroid(
                    &mut commands,
                    &mesh_handle,
                    &mut meshes,
                    &mut materials,
                    &transform,
                    Vec2::new(0., 1.),
                    &mut rng,
                );
            },
        );

        app.run();

        // Check that 2 splits were created
        // They should be located at (0, -25) and (0, 25)

        assert_eq!(app.world.query::<&Asteroid>().iter(&app.world).len(), 2);

        app.world
            .query::<(&Transform, &Asteroid)>()
            .iter(&app.world)
            .for_each(|(transform, _)| {
                let translation = transform.translation;
                assert!(translation.y == -25. || translation.y == 25.);
                assert_eq!(translation.x, 0.);
            });
    }
}
