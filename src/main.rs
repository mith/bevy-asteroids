mod asteroids;
mod edge_wrap;
mod input;
mod player;
mod ship;
mod split_mesh;
mod turret;
mod utils;

use asteroids::{
    despawn_asteroids, spawn_asteroids, Asteroid, ASTEROID_MAX_SPAWN_ANG_VELOCITY,
    ASTEROID_MAX_SPAWN_LIN_VELOCITY,
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
            RapierDebugRenderPlugin::default(),
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
    let mesh_collision_direction = transform
        .rotation
        .inverse()
        .mul_vec3(collision_direction.extend(0.))
        .truncate();

    let Some((mesh_a, mesh_b)) = split_mesh(mesh, mesh_collision_direction) else {
        return;
    };

    let aabb = mesh.compute_aabb().unwrap().half_extents.xy();
    let handle_a = meshes.add(mesh_a);
    let handle_b = meshes.add(mesh_b);

    // Create new transforms for the smaller asteroids
    let mut transform_a = *transform;
    let mut transform_b = *transform;
    // Move the new asteroids slightly away from each other, away from the collision plane.
    // Calculate the max size of the mesh along the normal of the collision plane
    // and move the new asteroids by that amount.
    let normal = Vec2::new(-collision_direction.y, collision_direction.x);

    let projection_x = aabb.x * normal.x.abs();
    let projection_y = aabb.y * normal.y.abs();
    let length = projection_x + projection_y;

    let offset = (normal * length).extend(0.) * 0.35;

    transform_a.translation += offset;
    transform_b.translation -= offset;

    // Spawn new asteroid entities
    spawn_asteroid_split(rng, commands, transform_a, meshes, materials, handle_a);
    spawn_asteroid_split(rng, commands, transform_b, meshes, materials, handle_b);
}

fn spawn_asteroid_split(
    rng: &mut ThreadRng,
    commands: &mut Commands,
    transform: Transform,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    mesh_handle: Handle<Mesh>,
) {
    let asteroid_velocity = Vec2::new(
        rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY),
        rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY),
    );
    let asteroid_angular_velocity =
        rng.gen_range(-ASTEROID_MAX_SPAWN_ANG_VELOCITY..ASTEROID_MAX_SPAWN_ANG_VELOCITY);

    let collider = mesh_to_collider(meshes.get(&mesh_handle).unwrap());

    commands.spawn((
        Asteroid,
        MaterialMesh2dBundle {
            transform,
            mesh: mesh_handle.into(),
            material: materials.add(ColorMaterial::from(Color::WHITE)),
            ..default()
        },
        RigidBody::Dynamic,
        collider,
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
        ActiveEvents::COLLISION_EVENTS,
        Duplicable,
    ));
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
