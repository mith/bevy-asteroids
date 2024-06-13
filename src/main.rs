#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod asteroid;
mod edge_wrap;
mod explosion;
mod game_state;
mod input;
mod mesh_utils;
mod player;
mod projectile;
mod shatter;
mod ship;
mod split_mesh;
mod turret;
mod ui;
mod utils;

use asteroid::{spawn_asteroids, Asteroid, AsteroidPlugin, AsteroidSet};
use bevy::{asset::AssetMetaCheck, prelude::*};
use bevy_rapier2d::prelude::{NoUserData, RapierConfiguration, RapierPhysicsPlugin};
use edge_wrap::{EdgeWrapPlugin, EdgeWrapSet};
use explosion::{Explosion, ExplosionPlugin};
use game_state::{GameResult, GameState};
use input::{PlayerInputPlugin, PlayerInputSet};
use player::{spawn_player, Player};
use projectile::{Projectile, ProjectilePlugin, ProjectileSet};
use shatter::{Debris, ShatterPlugin, ShatterSet};
use ship::{ShipDestroyedEvent, ShipPlugin, ShipSet};
use turret::{TurretPlugin, TurretSet};
use ui::{FinishedScreenPlugin, HudPlugin, StartScreenPlugin};
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
                // present_mode: PresentMode::Mailbox,
                title: "Asteroids".to_string(),
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
            ExplosionPlugin,
            AsteroidPlugin,
            ShatterPlugin,
            StartScreenPlugin,
            FinishedScreenPlugin,
            HudPlugin,
        ))
        .add_systems(Startup, setup_camera)
        .add_systems(OnEnter(GameState::Playing), spawn_player)
        .add_systems(OnEnter(GameState::Playing), spawn_asteroids)
        .add_systems(
            OnExit(GameState::Finished),
            cleanup_types!(Player, Asteroid, Debris, Projectile, Explosion),
        )
        .configure_sets(
            Update,
            (
                PlayerInputSet,
                ShipSet,
                EdgeWrapSet,
                TurretSet,
                ProjectileSet,
                (AsteroidSet, (ShatterSet, GameFlowSet)).chain(),
            )
                .chain(),
        )
        .add_systems(
            Update,
            ((player_destroyed, asteroids_cleared).run_if(in_state(GameState::Playing)))
                .in_set(GameFlowSet),
        );

    app.run();
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
struct GameFlowSet;

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}

fn player_destroyed(
    mut commands: Commands,
    mut next_gamestate: ResMut<NextState<GameState>>,
    mut ship_destroyed_events: EventReader<ShipDestroyedEvent>,
    player_query: Query<Entity, With<Player>>,
) {
    if !ship_destroyed_events.is_empty() && player_query.is_empty() {
        info!("Player destroyed");
        commands.insert_resource(GameResult::Lose);
        next_gamestate.set(GameState::Finished);
    }

    ship_destroyed_events.clear();
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
