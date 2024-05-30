mod asteroids;
mod edge_wrap;
mod input;
mod mesh_utils;
mod player;
mod projectile;
mod ship;
mod split_mesh;
mod turret;
mod utils;

use asteroids::{
    debris_lifetime, spawn_asteroids, split_asteroid_event, Asteroid, Debris, SplitAsteroidEvent,
};
use bevy::{asset::AssetMetaCheck, prelude::*, sprite::Mesh2dHandle};
use bevy_rapier2d::{
    dynamics::Velocity,
    prelude::{CollisionEvent, NoUserData, RapierConfiguration, RapierPhysicsPlugin},
};
use edge_wrap::{EdgeWrapPlugin, EdgeWrapSet};
use input::{PlayerInputPlugin, PlayerInputSet};
use player::{spawn_player, Player};
use projectile::{
    explosion_expansion, projectile_asteroid_collision, projectile_explosion, projectile_timer,
    Projectile, ProjectileExplosionEvent,
};
use ship::ship_movement;
use turret::{fire_projectile, reload, FireEvent};

use crate::asteroids::spawn_shattered_mesh;

const PHYSICS_LENGTH_UNIT: f32 = 100.0;

macro_rules! cleanup_types {
    ( $( $type:ty ),* ) => {
        (
            $(
                cleanup::<$type>,
            )*
        )
    };
}

fn main() {
    let mut app = App::new();

    let mut rapier_configuration = RapierConfiguration::new(PHYSICS_LENGTH_UNIT);
    rapier_configuration.gravity = Vec2::new(0., 0.);

    app.insert_resource(ClearColor(Color::BLACK))
        .insert_resource(AssetMetaCheck::Never)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
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
        .add_systems(OnEnter(GameState::Menu), show_menu_ui)
        .add_systems(OnExit(GameState::Menu), cleanup_types!(Menu))
        .add_systems(OnEnter(GameState::Playing), spawn_player)
        .add_systems(OnEnter(GameState::Playing), spawn_asteroids)
        .add_systems(
            OnExit(GameState::Finished),
            cleanup_types!(Player, Asteroid, Debris, Projectile),
        )
        .add_systems(OnEnter(GameState::Finished), show_game_finished)
        .add_systems(OnExit(GameState::Finished), clear_game_result)
        .add_systems(Update, start_game.run_if(in_state(GameState::Menu)))
        .configure_sets(Update, (PlayerInputSet, EdgeWrapSet).chain())
        .add_event::<FireEvent>()
        .add_event::<ProjectileExplosionEvent>()
        .add_event::<SplitAsteroidEvent>()
        .add_systems(
            Update,
            (
                reload,
                projectile_timer,
                apply_deferred,
                fire_projectile,
                projectile_asteroid_collision,
                split_asteroid_event,
                projectile_explosion,
                explosion_expansion,
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
    Menu,
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
struct Menu;

const FONT_PATH: &str = "fonts/TurretRoad-ExtraLight.ttf";

fn show_menu_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Name::new("Menu screen"),
            Menu,
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
                Name::new("Title text"),
                TextBundle::from_section(
                    "Asteroids",
                    TextStyle {
                        font: asset_server.load(FONT_PATH),
                        font_size: 90.,
                        color: Color::WHITE,
                    },
                ),
            ));
            parent.spawn((
                Name::new("Start text"),
                TextBundle::from_section(
                    "Press Space to start",
                    TextStyle {
                        font: asset_server.load(FONT_PATH),
                        font_size: 40.,
                        color: Color::WHITE,
                    },
                ),
            ));
        });
}

#[derive(Component)]
struct FinishedText;

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

fn start_game(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut next_gamestate: ResMut<NextState<GameState>>,
) {
    if keyboard_input.pressed(KeyCode::Space) {
        next_gamestate.set(GameState::Playing);
        info!("Starting game");
    }
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
