mod asteroid;
mod edge_wrap;
mod game_state;
mod input;
mod mesh_utils;
mod player;
mod projectile;
mod ship;
mod split_mesh;
mod turret;
mod ui;
mod utils;

use asteroid::{spawn_asteroids, Asteroid, AsteroidPlugin, AsteroidSet, Debris};
use bevy::{asset::AssetMetaCheck, prelude::*};
use bevy_rapier2d::prelude::{NoUserData, RapierConfiguration, RapierPhysicsPlugin};
use edge_wrap::{EdgeWrapPlugin, EdgeWrapSet};
use game_state::{GameResult, GameState};
use input::{PlayerInputPlugin, PlayerInputSet};
use player::{spawn_player, Player};
use projectile::{Projectile, ProjectilePlugin, ProjectileSet};
use ship::{ShipDestroyedEvent, ShipPlugin, ShipSet};
use turret::{TurretPlugin, TurretSet};
use ui::{FinishedScreenPlugin, StartScreenPlugin};
use utils::cleanup;

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
        .add_plugins((
            EdgeWrapPlugin,
            PlayerInputPlugin,
            ShipPlugin,
            TurretPlugin,
            ProjectilePlugin,
            AsteroidPlugin,
            StartScreenPlugin,
            FinishedScreenPlugin,
        ))
        .add_systems(Startup, setup_camera)
        .add_systems(OnEnter(GameState::Playing), spawn_player)
        .add_systems(OnEnter(GameState::Playing), spawn_asteroids)
        .add_systems(
            OnExit(GameState::Finished),
            cleanup_types!(Player, Asteroid, Debris, Projectile),
        )
        .configure_sets(
            Update,
            (
                PlayerInputSet,
                EdgeWrapSet,
                ShipSet,
                TurretSet,
                ProjectileSet,
                AsteroidSet,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                (player_destroyed, asteroids_cleared).run_if(in_state(GameState::Playing)),
                restart_game.run_if(in_state(GameState::Finished)),
            )
                .chain(),
        );

    app.run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}

fn player_destroyed(
    mut commands: Commands,
    mut next_gamestate: ResMut<NextState<GameState>>,
    mut ship_destroyed_events: EventReader<ShipDestroyedEvent>,
    player_query: Query<Entity, With<Player>>,
) {
    for _ in ship_destroyed_events.read() {
        if player_query.is_empty() {
            info!("Player destroyed");
            commands.insert_resource(GameResult::Lose);
            next_gamestate.set(GameState::Finished);
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
