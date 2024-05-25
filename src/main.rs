mod asteroids;
mod edge_wrap;
mod input;
mod mesh_utils;
mod player;
mod ship;
mod split_mesh;
mod turret;
mod utils;

use asteroids::{spawn_asteroids, Asteroid, ASTEROID_MAX_SPAWN_ANG_VELOCITY, FROZEN_ASTEROIDS};
use bevy::{
    prelude::*,
    sprite::{MaterialMesh2dBundle, Mesh2dHandle},
    window::WindowMode,
};
use bevy_rapier2d::{
    dynamics::{RigidBody, Sleeping, Velocity},
    geometry::{ActiveEvents, ColliderDisabled, Restitution},
    plugin::RapierContext,
    prelude::{CollisionEvent, NoUserData, RapierConfiguration, RapierPhysicsPlugin},
};
use edge_wrap::{Duplicable, EdgeWrapPlugin, EdgeWrapSet};
use input::{PlayerInputPlugin, PlayerInputSet};
use mesh_utils::calculate_mesh_area;
use player::{spawn_player, Player};
use rand::{rngs::ThreadRng, Rng};
use ship::ship_movement;
use split_mesh::{shatter_mesh, split_mesh, trim_mesh};
use turret::{fire_projectile, projectile_timer, reload, FireEvent, Projectile};
use utils::mesh_to_collider;

const PHYSICS_LENGTH_UNIT: f32 = 100.0;

fn main() {
    let mut app = App::new();

    let mut rapier_configuration = RapierConfiguration::new(PHYSICS_LENGTH_UNIT);
    rapier_configuration.gravity = Vec2::new(0., 0.);

    app.insert_resource(ClearColor(Color::BLACK))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                mode: WindowMode::BorderlessFullscreen,
                canvas: Some("#game".to_string()),
                ..default()
            }),
            ..default()
        }))
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
            (
                cleanup::<Player>,
                cleanup::<Asteroid>,
                cleanup::<Debris>,
                cleanup::<Projectile>,
            ),
        )
        .add_systems(OnEnter(GameState::Finished), show_game_finished)
        .add_systems(OnExit(GameState::Finished), clear_game_result)
        .configure_sets(Update, (PlayerInputSet, EdgeWrapSet).chain())
        .add_event::<FireEvent>()
        .add_systems(
            Update,
            (
                reload,
                projectile_timer,
                apply_deferred,
                fire_projectile,
                projectile_asteroid_collision,
                apply_deferred,
                debris_lifetime,
                asteroids_cleared.run_if(in_state(GameState::Playing)),
            )
                .chain()
                .after(PlayerInputSet),
        )
        .add_systems(
            Update,
            (ship_movement, apply_deferred, player_asteroid_collision)
                .chain()
                .after(EdgeWrapSet),
        )
        .add_systems(Update, restart_game.run_if(in_state(GameState::Finished)));

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

#[derive(Resource)]
enum GameResult {
    Win,
    Lose,
}

fn player_asteroid_collision(
    mut commands: Commands,
    mut collision_events: EventReader<CollisionEvent>,
    player_query: Query<(Entity, &Transform, Option<&Velocity>, &mut Mesh2dHandle), With<Player>>,
    mut next_gamestate: ResMut<NextState<GameState>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for event in collision_events.read() {
        if let CollisionEvent::Started(entity_a, entity_b, _) = event {
            let Some((player_entity, player_transform, player_velocity, player_mesh_handle)) =
                player_query.get_single().ok()
            else {
                return;
            };
            if player_entity == *entity_a || player_entity == *entity_b {
                info!("Player collided with asteroid");
                commands.insert_resource(GameResult::Lose);
                next_gamestate.set(GameState::Finished);

                let mesh = meshes
                    .get(&player_mesh_handle.0)
                    .expect("Player mesh not found")
                    .clone();

                spawn_shattered_mesh(
                    &mesh,
                    player_transform,
                    player_velocity.copied().unwrap_or_else(Velocity::zero),
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                );

                commands.entity(player_entity).despawn_recursive();
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
    collision_position: Vec2,
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

    let [(mesh_a, offset_a), (mesh_b, offset_b)] =
        split_mesh(mesh, mesh_collision_direction, collision_position);

    let min_area = 500.;

    // Calculate area of the split mesh
    // Skip spawning if the area of the split mesh is too small

    let mut spawn = |mesh: &Mesh, offset: Vec2| {
        // Check if mesh contains any vertices
        if mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .map(|attr| attr.len() < 3)
            .unwrap_or(true)
        {
            error!("Mesh has no triangles, skipping split");
            return;
        }
        let trimmed = trim_mesh(mesh);
        let translation = transform.transform_point((offset + trimmed.0 .1).extend(0.));
        let main_transform =
            Transform::from_translation(translation).with_rotation(transform.rotation);
        let velocity = Velocity {
            linvel: velocity.linvel
                + asteroid_rotation
                    .mul_vec3(offset.extend(0.))
                    .truncate()
                    .normalize()
                    * 50.,
            angvel: velocity.angvel,
        };
        if calculate_mesh_area(mesh) > min_area {
            // let mesh = round_mesh(mesh).0;
            spawn_asteroid_split(
                commands,
                main_transform,
                velocity,
                meshes,
                materials,
                &trimmed.0 .0,
            );
        } else {
            spawn_shattered_mesh(mesh, &main_transform, velocity, commands, meshes, materials);
        }

        for (mesh, trimmed_offset) in trimmed.1 {
            let translation = transform.transform_point((offset + trimmed_offset).extend(0.));
            let transform =
                Transform::from_translation(translation).with_rotation(main_transform.rotation);
            spawn_shattered_mesh(&mesh, &transform, velocity, commands, meshes, materials);
        }
    };

    spawn(&mesh_a, offset_a);
    spawn(&mesh_b, offset_b);
}

fn spawn_shattered_mesh(
    mesh: &Mesh,
    transform: &Transform,
    velocity: Velocity,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) {
    let mut rng = ThreadRng::default();
    let shards = shatter_mesh(mesh, 12.);
    for (mesh, offset) in shards.iter() {
        let shard_translation = transform.transform_point(offset.extend(0.));
        let shard_transform =
            Transform::from_translation(shard_translation).with_rotation(transform.rotation);

        let rng_range_max = 5.;

        let velocity = Velocity {
            linvel: transform
                .rotation
                .mul_vec3(offset.extend(0.))
                .normalize()
                .xy()
                * 15.
                + velocity.linvel
                + Vec2::new(
                    rng.gen_range(-rng_range_max..rng_range_max),
                    rng.gen_range(-rng_range_max..rng_range_max),
                ),
            angvel: rng
                .gen_range(-ASTEROID_MAX_SPAWN_ANG_VELOCITY..ASTEROID_MAX_SPAWN_ANG_VELOCITY),
        };

        spawn_debris(commands, shard_transform, velocity, meshes, materials, mesh)
    }
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
    let mut rng = ThreadRng::default();

    let mut asteroid_cmd = commands.spawn((
        Debris {
            lifetime: Timer::from_seconds(rng.gen_range(0.5..5.0), TimerMode::Once),
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
    rapier_context: Res<RapierContext>,
    mut collision_events: EventReader<CollisionEvent>,
    projectile_query: Query<&Projectile>,
    mut asteroid_query: Query<(&Transform, &mut Mesh2dHandle, Option<&Velocity>), With<Asteroid>>,
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

            commands.entity(projectile_entity).despawn();
            // Split asteroid into smaller asteroids
            if let Ok((transform, mesh_handle, velocity)) = asteroid_query.get_mut(asteroid_entity)
            {
                let contact = rapier_context
                    .contact_pair(projectile_entity, asteroid_entity)
                    .expect("No contact found for projectile-asteroid collision");
                if !contact.has_any_active_contacts() {
                    continue;
                }
                let (contact_manifold, contact) = contact
                    .find_deepest_contact()
                    .expect("No contact point found for projectile-asteroid collision");
                split_asteroid(
                    &mut commands,
                    &mesh_handle.0,
                    &mut meshes,
                    &mut materials,
                    transform,
                    velocity.copied().unwrap_or_else(Velocity::zero),
                    contact_manifold.normal(),
                    contact.local_p2(),
                );
                commands.entity(asteroid_entity).despawn();
            }
        }
    }
}

fn asteroids_cleared(
    mut commands: Commands,
    asteroid_query: Query<Entity, With<Asteroid>>,
    mut next_gamestate: ResMut<NextState<GameState>>,
) {
    if asteroid_query.iter().count() == 0 {
        info!("All asteroids cleared");
        commands.insert_resource(GameResult::Win);
        next_gamestate.set(GameState::Finished);
    }
}

#[derive(Component)]
struct FinishedText;

const FONT_PATH: &str = "fonts/TurretRoad-ExtraLight.ttf";

fn show_game_finished(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    game_result: Res<GameResult>,
) {
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
                    match *game_result {
                        GameResult::Win => "You win!",
                        GameResult::Lose => "Game over!",
                    },
                    TextStyle {
                        font: asset_server.load(FONT_PATH),
                        font_size: 90.,
                        color: Color::WHITE,
                    },
                ),
            ));
            parent.spawn((
                Name::new("Restart text"),
                TextBundle::from_section(
                    "Press R to restart",
                    TextStyle {
                        font: asset_server.load(FONT_PATH),
                        font_size: 40.,
                        color: Color::WHITE,
                    },
                ),
            ));
        });
}

fn restart_game(
    mut commands: Commands,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut next_gamestate: ResMut<NextState<GameState>>,
) {
    if keyboard_input.pressed(KeyCode::KeyR) {
        commands.remove_resource::<GameResult>();
        next_gamestate.set(GameState::Playing);
        info!("Restarting game");
    }
}

fn cleanup<T: Component>(mut commands: Commands, query: Query<Entity, With<T>>) {
    for entity in &query {
        commands.entity(entity).despawn_recursive();
    }
}

fn clear_game_result(mut commands: Commands, finish_text_query: Query<Entity, With<FinishedText>>) {
    for finished_text_entity in &finish_text_query {
        commands.entity(finished_text_entity).despawn_recursive();
    }
}

#[cfg(test)]
mod tests {

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
                    Vec2::ZERO,
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
                    Vec2::ZERO,
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
