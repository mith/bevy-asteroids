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
    geometry::{ActiveEvents, ColliderDisabled, Restitution},
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
use split_mesh::{shatter_mesh, split_mesh};
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
                debris_lifetime,
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

#[derive(Component)]
struct Debris {
    lifetime: Timer,
}

fn split_asteroid(
    commands: &mut Commands,
    original_mesh: &Handle<Mesh>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    transform: &Transform,
    velocity: Velocity,
    collision_direction: Vec2,
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

    let [(mesh_a, offset_a), (mesh_b, offset_b)] = split_mesh(mesh, mesh_collision_direction);

    let min_area = 500.;

    // Calculate area of the split mesh
    // Skip spawning if the area of the split mesh is too small

    let mut rng = ThreadRng::default();

    let mut spawn = |mesh: &Mesh, offset: Vec2| {
        let translation = transform.transform_point(offset.extend(0.));
        let transform = Transform::from_translation(translation).with_rotation(transform.rotation);
        if calculate_mesh_area(mesh) > min_area {
            spawn_asteroid_split(commands, transform, velocity, meshes, materials, mesh);
        } else {
            let shards = shatter_mesh(mesh, 2);
            for (mesh, offset) in shards.iter() {
                let shard_translation = transform.transform_point(offset.extend(0.));
                let shard_transform = Transform::from_translation(shard_translation)
                    .with_rotation(transform.rotation);

                let velocity = Velocity {
                    linvel: transform
                        .rotation
                        .mul_vec3(offset.extend(0.))
                        .normalize()
                        .xy()
                        * 15.
                        + velocity.linvel,
                    angvel: rng.gen_range(
                        -ASTEROID_MAX_SPAWN_ANG_VELOCITY..ASTEROID_MAX_SPAWN_ANG_VELOCITY,
                    ),
                };

                spawn_debris(commands, shard_transform, velocity, meshes, materials, mesh)
            }
        }
    };

    spawn(&mesh_a, offset_a);
    spawn(&mesh_b, offset_b);
}

fn spawn_asteroid_split(
    commands: &mut Commands,
    transform: Transform,
    velocity: Velocity,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    mesh: &Mesh,
) {
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
            velocity,
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

fn spawn_debris(
    commands: &mut Commands,
    transform: Transform,
    velocity: Velocity,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    mesh: &Mesh,
) {
    let collider = mesh_to_collider(mesh);

    let mut asteroid_cmd = commands.spawn((
        Debris {
            lifetime: Timer::from_seconds(3.0, TimerMode::Once),
        },
        MaterialMesh2dBundle {
            transform,
            mesh: Mesh2dHandle(meshes.add(mesh.clone())),
            material: materials.add(ColorMaterial::from(Color::WHITE)),
            ..default()
        },
        collider,
        Duplicable,
        ColliderDisabled,
    ));
    if !FROZEN_ASTEROIDS {
        asteroid_cmd.insert((
            RigidBody::Dynamic,
            velocity,
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

fn debris_lifetime(
    mut commands: Commands,
    time: Res<Time>,
    mut debris_query: Query<(Entity, &mut Debris)>,
) {
    for (entity, mut debris) in &mut debris_query {
        debris.lifetime.tick(time.delta());
        if debris.lifetime.finished() {
            commands.entity(entity).despawn();
        }
    }
}

fn projectile_asteroid_collision(
    mut commands: Commands,
    mut collision_events: EventReader<CollisionEvent>,
    projectile_query: Query<&Projectile>,
    mut asteroid_query: Query<(&Transform, &mut Mesh2dHandle, &Velocity), With<Asteroid>>,
    transform_query: Query<&Transform>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
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
            if let Ok((transform, mesh_handle, velocity)) = asteroid_query.get_mut(asteroid_entity)
            {
                split_asteroid(
                    &mut commands,
                    &mesh_handle.0,
                    &mut meshes,
                    &mut materials,
                    transform,
                    *velocity,
                    collision_direction.xy().normalize(),
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

                split_asteroid(
                    &mut commands,
                    &mesh_handle,
                    &mut meshes,
                    &mut materials,
                    &transform,
                    Velocity::zero(),
                    Vec2::new(0., 1.),
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

                split_asteroid(
                    &mut commands,
                    &mesh_handle,
                    &mut meshes,
                    &mut materials,
                    &transform,
                    Velocity::zero(),
                    Vec2::new(0., 1.),
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
