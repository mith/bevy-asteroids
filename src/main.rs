mod edge_wrap;
mod utils;

use bevy::{
    math::Vec3Swizzles,
    prelude::*,
    render::mesh::VertexAttributeValues,
    sprite::{MaterialMesh2dBundle, Mesh2dHandle},
};
use bevy_rapier2d::{
    na::{Isometry2, Vector2},
    prelude::*,
};
use edge_wrap::{Duplicable, EdgeWrapPlugin, EdgeWrapSet};
use rand::Rng;
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
        .add_plugins(EdgeWrapPlugin)
        .add_systems(Startup, setup_camera)
        .add_systems(OnEnter(GameState::Playing), spawn_player)
        .add_systems(OnEnter(GameState::Playing), spawn_asteroids)
        .add_systems(
            OnExit(GameState::Finished),
            (despawn_player, despawn_asteroids),
        )
        .add_systems(OnEnter(GameState::Finished), show_game_finished)
        .add_systems(OnExit(GameState::Finished), clear_game_result)
        .add_systems(
            Update,
            (
                player_ship_input.run_if(in_state(GameState::Playing)),
                stop_ship.run_if(not(in_state(GameState::Playing))),
                apply_deferred,
            )
                .chain()
                .before(EdgeWrapSet),
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

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Ship;

#[derive(Component)]
struct Thruster;

#[derive(Component)]
struct Throttling;

fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    spawn_player_ship(
        &mut commands,
        &mut meshes,
        &mut materials,
        Transform::default(),
    );
}

fn spawn_player_ship(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    transform: Transform,
) {
    let player_shape = RegularPolygon::new(10., 3);

    let player_mesh = Mesh::from(player_shape);
    let collider = mesh_to_collider(&player_mesh);
    commands
        .spawn((
            Name::new("Player"),
            Player,
            Ship,
            MaterialMesh2dBundle {
                mesh: meshes.add(player_mesh).into(),
                material: materials.add(ColorMaterial::from(Color::WHITE)),
                transform,
                ..default()
            },
            RigidBody::Dynamic,
            collider,
            Duplicable,
        ))
        .with_children(|parent| {
            parent.spawn((
                Name::new("Thruster"),
                Thruster,
                MaterialMesh2dBundle {
                    transform: Transform::from_translation(Vec3::new(0., -10., 0.)),
                    mesh: meshes.add(Mesh::from(RegularPolygon::new(5., 3))).into(),
                    material: materials.add(ColorMaterial::from(Color::RED)),
                    visibility: Visibility::Hidden,
                    ..default()
                },
            ));
        });
}

fn player_ship_input(
    mut commands: Commands,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut player_query: Query<(Entity, &GlobalTransform, &mut Transform), (With<Player>, With<Ship>)>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window>,
) {
    let (camera, camera_global_transform) = camera_query.single();
    let Some(cursor_pos) = windows
        .single()
        .cursor_position()
        .and_then(|cp| camera.viewport_to_world_2d(camera_global_transform, cp))
    else {
        return;
    };

    for (player_entity, player_global_transform, mut player_transform) in player_query.iter_mut() {
        let throttle =
            keyboard_input.pressed(KeyCode::Space) || mouse_input.pressed(MouseButton::Left);

        if throttle {
            commands.entity(player_entity).insert(Throttling);
        } else {
            commands.entity(player_entity).remove::<Throttling>();
        }

        let direction = cursor_pos - player_global_transform.translation().truncate();
        let angle = direction.y.atan2(direction.x);
        let target_rotation = Quat::from_rotation_z(angle - std::f32::consts::FRAC_PI_2);

        player_transform.rotation = target_rotation;
    }
}

fn stop_ship(mut commands: Commands, player_query: Query<Entity, With<Player>>) {
    for player_entity in player_query.iter() {
        commands.entity(player_entity).remove::<Throttling>();
    }
}

fn ship_movement(
    mut commands: Commands,
    ship_query: Query<(Entity, &Transform, Option<&Throttling>, &Children), With<Ship>>,
    mut thruster_query: Query<&mut Handle<ColorMaterial>, With<Thruster>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    time: Res<Time>,
) {
    let ship_power = 500.;
    let thruster_fade_speed = 1.;

    for (ship_entity, global_transform, throttling, children) in &ship_query {
        let thruster_entity = children
            .iter()
            .find(|child_entity| thruster_query.contains(**child_entity))
            .copied()
            .unwrap();

        let thruster_material_handle = thruster_query.get_mut(thruster_entity).unwrap();

        let thruster_material = materials.get_mut(thruster_material_handle.clone()).unwrap();

        if throttling.is_some() {
            let force = global_transform
                .rotation
                .mul_vec3(Vec3::new(0., 1., 0.))
                .xy();

            commands.entity(ship_entity).insert(ExternalImpulse {
                impulse: force * ship_power,
                ..default()
            });
            commands.entity(thruster_entity).insert(Visibility::Visible);

            let thruster_transparency = thruster_material.color.a();
            thruster_material.color = thruster_material
                .color
                .with_a(thruster_transparency.lerp(1., time.delta_seconds() * thruster_fade_speed));
        } else {
            commands.entity(ship_entity).remove::<ExternalImpulse>();
            commands.entity(thruster_entity).insert(Visibility::Hidden);

            let thruster_transparency = thruster_material.color.a();
            thruster_material.color = thruster_material.color.with_a(
                thruster_transparency.lerp(0., time.delta_seconds() * thruster_fade_speed * 2.),
            );
        }
    }
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

#[derive(Component)]
struct Asteroid {
    splits_left: u8,
}

const ASTEROID_SPAWN_COUNT: usize = 10;
const ASTEROID_MAX_VERTICE_DRIFT: f32 = 10.;
const ASTEROID_MAX_SPAWN_LIN_VELOCITY: f32 = 50.;
const ASTEROID_MAX_SPAWN_ANG_VELOCITY: f32 = 1.;

fn spawn_asteroids(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mut asteroid_positions: Vec<Vec2> = Vec::new();
    while asteroid_positions.len() < ASTEROID_SPAWN_COUNT {
        let mut rng = rand::thread_rng();
        let max_x = 500.;
        let max_y = 500.;
        let asteroid_pos = Vec2::new(
            rng.gen_range(-max_x..max_x), // x
            rng.gen_range(-max_y..max_y), // y
        );

        // skip spawning asteroids on top of player
        if asteroid_pos.length() < 100. {
            continue;
        }

        // skip spawning asteroids on top of other asteroids
        if asteroid_positions
            .iter()
            .any(|&pos| (pos - asteroid_pos).length() < 100.)
        {
            continue;
        }

        asteroid_positions.push(asteroid_pos);

        let asteroid_velocity = Vec2::new(
            rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY), // x
            rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY), // y
        );
        let asteroid_angular_velocity =
            rng.gen_range(-ASTEROID_MAX_SPAWN_ANG_VELOCITY..ASTEROID_MAX_SPAWN_ANG_VELOCITY);

        let asteroid_shape = RegularPolygon::new(50., 10);
        let mut asteroid_mesh = Mesh::from(asteroid_shape);

        let pos_attributes = asteroid_mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION).expect(
            "Mesh does not have a position attribute. This should not happen as we just created the mesh",
        );

        let VertexAttributeValues::Float32x3(pos_attr_vec3) = pos_attributes else {
            panic!("Position attribute is not a Float32x3");
        };

        pos_attr_vec3.iter_mut().for_each(|v| {
            // Translate vertice randomly
            v[0] += rng.gen_range(-ASTEROID_MAX_VERTICE_DRIFT..ASTEROID_MAX_VERTICE_DRIFT);
            v[1] += rng.gen_range(-ASTEROID_MAX_VERTICE_DRIFT..ASTEROID_MAX_VERTICE_DRIFT);
        });

        let collider = mesh_to_collider(&asteroid_mesh);

        commands.spawn((
            Asteroid { splits_left: 2 },
            MaterialMesh2dBundle {
                transform: Transform::default().with_translation(Vec3::new(
                    asteroid_pos.x,
                    asteroid_pos.y,
                    0.,
                )),
                mesh: meshes.add(asteroid_mesh).into(),
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
}

fn despawn_player(mut commands: Commands, mut player_query: Query<Entity, With<Player>>) {
    for player_entity in player_query.iter_mut() {
        commands.entity(player_entity).despawn_recursive();
    }
}

fn despawn_asteroids(mut commands: Commands, mut asteroid_query: Query<Entity, With<Asteroid>>) {
    for asteroid_entity in asteroid_query.iter_mut() {
        commands.entity(asteroid_entity).despawn_recursive();
    }
}
