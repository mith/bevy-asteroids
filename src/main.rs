use bevy::{math::Vec3Swizzles, prelude::*, sprite::MaterialMesh2dBundle};
use bevy_rapier2d::prelude::*;
use rand::Rng;

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
        .add_systems(Startup, setup_camera)
        .add_systems(OnEnter(GameState::Playing), (spawn_player, spawn_asteroids))
        .add_systems(
            OnExit(GameState::Finished),
            (despawn_player, despawn_asteroids),
        )
        .add_systems(OnEnter(GameState::Finished), show_game_finished)
        .add_systems(OnExit(GameState::Finished), clear_game_result)
        .add_systems(
            Update,
            (
                player_ship_input,
                ship_movement,
                teleport_on_map_edge,
                player_asteroid_collision,
            )
                .run_if(in_state(GameState::Playing))
                .chain(),
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
struct Throttling;

fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let player_shape = RegularPolygon::new(10., 3);

    let player_mesh = Mesh::from(player_shape);
    let collider = mesh_to_collider(&player_mesh);
    commands.spawn((
        Player,
        Ship,
        MaterialMesh2dBundle {
            mesh: meshes.add(player_mesh).into(),
            material: materials.add(ColorMaterial::from(Color::WHITE)),
            ..default()
        },
        RigidBody::Dynamic,
        collider,
    ));
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

fn ship_movement(
    mut commands: Commands,
    ship_query: Query<(Entity, &Transform, Option<&Throttling>), With<Ship>>,
) {
    for (ship_entity, global_transform, throttling) in &ship_query {
        if throttling.is_some() {
            let force = global_transform
                .rotation
                .mul_vec3(Vec3::new(0., 1., 0.))
                .xy();

            let ship_power = 500.;
            commands.entity(ship_entity).insert(ExternalImpulse {
                impulse: force * ship_power,
                ..default()
            });
        } else {
            commands.entity(ship_entity).remove::<ExternalImpulse>();
        }
    }
}

fn teleport_on_map_edge(mut query: Query<&mut Transform, With<RigidBody>>) {
    for mut transform in &mut query {
        let pos = transform.translation;
        let bounds = 500.;
        if pos.x.abs() > bounds {
            transform.translation.x = pos.x - (bounds * 2. * pos.x.signum());
        }
        if pos.y.abs() > 500. {
            transform.translation.y = pos.y - (bounds * 2. * pos.y.signum());
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

fn mesh_to_collider(mesh: &Mesh) -> Collider {
    let vertices = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .unwrap()
        .as_float3()
        .unwrap()
        .to_vec()
        .iter()
        .map(|pos| Vec2::new(pos[0], pos[1]))
        .collect::<_>();
    let indices_vec = mesh
        .indices()
        .unwrap()
        .iter()
        .map(|i| i as u32)
        .collect::<Vec<u32>>()
        .chunks(3)
        .map(|chunk| [chunk[0], chunk[1], chunk[2]])
        .collect::<_>();
    Collider::trimesh(vertices, indices_vec)
}

#[derive(Component)]
struct Asteroid;

fn spawn_asteroids(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // spawn 10 asteroids of random size and position
    for _ in 0..10 {
        let mut rng = rand::thread_rng();
        let asteroid_size = Vec2::new(50., 50.);
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
        let asteroid_velocity = Vec2::new(
            rng.gen_range(-1.0..1.0), // x
            rng.gen_range(-1.0..1.0), // y
        );
        let asteroid_angular_velocity = rng.gen_range(-1.0..1.0);
        commands.spawn((
            Asteroid,
            MaterialMesh2dBundle {
                transform: Transform::default().with_translation(Vec3::new(
                    asteroid_pos.x,
                    asteroid_pos.y,
                    0.,
                )),
                mesh: meshes
                    .add(Mesh::from(shape::Quad::new(asteroid_size)))
                    .into(),
                material: materials.add(ColorMaterial::from(Color::WHITE)),
                ..default()
            },
            RigidBody::Dynamic,
            Collider::cuboid(asteroid_size.x * 0.5, asteroid_size.y * 0.5),
            Velocity {
                linvel: asteroid_velocity,
                angvel: asteroid_angular_velocity,
                ..default()
            },
            Restitution {
                coefficient: 0.9,
                ..default()
            },
            ActiveEvents::COLLISION_EVENTS,
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
