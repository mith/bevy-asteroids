mod movement;
mod tractor_beam;

use bevy::{
    app::{App, Plugin, Startup, Update},
    asset::{Asset, AssetServer, Assets, Handle},
    ecs::{
        component::Component,
        entity::Entity,
        event::{Event, EventReader, EventWriter},
        query::With,
        schedule::{
            common_conditions::{in_state, not, resource_exists},
            Condition, IntoSystemConfigs, OnEnter, SystemSet,
        },
        system::{Commands, Query, Res, ResMut, Resource},
    },
    hierarchy::DespawnRecursiveExt,
    input::{keyboard::KeyCode, ButtonInput},
    math::{Quat, Rect, Vec2, Vec3, Vec3Swizzles},
    prelude::default,
    reflect::TypePath,
    render::{color::Color, mesh::Mesh},
    sprite::{ColorMaterial, MaterialMesh2dBundle},
    time::{Time, Timer, TimerMode},
    transform::components::{GlobalTransform, Transform},
};
use bevy_common_assets::ron::RonAssetPlugin;
use bevy_rapier2d::{
    dynamics::{LockedAxes, RigidBody, Velocity},
    geometry::{CollisionGroups, Group},
};
use movement::{move_ufo, AvoidanceWeights};
use rand::Rng;
use serde::Deserialize;
use tracing::info;
use tractor_beam::{throw_asteroid, TractorBeam};

use crate::{
    asteroid::SplitAsteroidEvent,
    edge_wrap::{Bounds, Duplicable},
    explosion,
    game_state::GameState,
    player::Player,
    projectile::PROJECTILE_GROUP,
    shatter::spawn_shattered_mesh,
    utils::mesh_to_collider,
};

pub struct UfoPlugin;

impl Plugin for UfoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_ufo_assets)
            .add_event::<UfoDestroyedEvent>()
            .add_plugins(RonAssetPlugin::<UfoSettings>::new(&["ufo_settings.ron"]))
            .add_systems(Startup, load_ufo_settings)
            .init_resource::<SpawnTimer>()
            .add_systems(OnEnter(GameState::Playing), reset_spawn_timer)
            .add_systems(
                Update,
                (
                    move_ufo,
                    ufo_inside_bounds,
                    throw_asteroid,
                    ufo_destroyed,
                    spawn_ufo,
                )
                    .run_if(not(in_state(GameState::Menu)))
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    set_ufo_settings_resource.run_if(
                        resource_exists::<UfoSettingsHandle>
                            .and_then(not(resource_exists::<UfoSettings>)),
                    ),
                    toggle_debug.run_if(resource_exists::<UfoSettings>),
                ),
            );
    }
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct UfoSet;

#[derive(Component)]
pub struct Ufo;

#[derive(Component)]
pub struct KillTarget(Entity);

#[derive(Resource, Debug, Default, Deserialize, Asset, TypePath, Clone)]
struct UfoSettings {
    debug_enabled: bool,
    max_acceleration: f32,
    max_velocity: f32,
    avoidance_weights: AvoidanceWeights,
}

#[derive(Resource)]
struct UfoSettingsHandle(Handle<UfoSettings>);

fn load_ufo_settings(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(UfoSettingsHandle(asset_server.load("ufo_settings.ron")));
}

fn set_ufo_settings_resource(
    mut commands: Commands,
    ufo_settings_handle: Res<UfoSettingsHandle>,
    ufo_settings_assets: Res<Assets<UfoSettings>>,
) {
    if let Some(ufo_settings) = ufo_settings_assets.get(ufo_settings_handle.0.clone()) {
        commands.insert_resource(ufo_settings.clone());
    }
}

fn toggle_debug(mut ufo_settings: ResMut<UfoSettings>, keyboard_input: Res<ButtonInput<KeyCode>>) {
    if keyboard_input.just_pressed(KeyCode::F3) {
        ufo_settings.debug_enabled = !ufo_settings.debug_enabled;
    }
}

#[derive(Resource)]
struct UfoAssets {
    ufo_mesh: Handle<Mesh>,
    ufo_material: Handle<ColorMaterial>,
}

fn load_ufo_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.insert_resource(UfoAssets {
        ufo_mesh: asset_server.load("meshes/ufo.glb#Mesh0/Primitive0"),
        ufo_material: materials.add(ColorMaterial::from(Color::WHITE)),
    });
}

pub const UFO_GROUP: Group = Group::GROUP_5;

#[derive(Resource)]
struct SpawnTimer {
    timer: Timer,
}

impl Default for SpawnTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(30., TimerMode::Once),
        }
    }
}

fn reset_spawn_timer(mut spawn_timer: ResMut<SpawnTimer>) {
    spawn_timer.timer.reset();
    spawn_timer.timer.pause();
}

fn spawn_ufo(
    mut commands: Commands,
    ufo_assets: Res<UfoAssets>,
    meshes: Res<Assets<Mesh>>,
    ufo_query: Query<Entity, With<Ufo>>,
    player_query: Query<Entity, With<Player>>,
    mut split_asteroid_events: EventReader<SplitAsteroidEvent>,
    bounds: Res<Bounds>,
    mut spawn_timer: ResMut<SpawnTimer>,
    ufo_settings: Res<UfoSettings>,
    time: Res<Time>,
) {
    if !ufo_query.is_empty() {
        return;
    }

    if spawn_timer.timer.paused() {
        spawn_timer.timer.unpause();
    }

    if !spawn_timer.timer.tick(time.delta()).finished() {
        return;
    }

    let Ok(player_entity) = player_query.get_single() else {
        return;
    };

    let mut rng = rand::thread_rng();

    for _event in split_asteroid_events.read() {
        if rng.gen_bool(0.3) {
            continue;
        }
        info!("Spawning UFO");
        let mesh = meshes
            .get(&ufo_assets.ufo_mesh)
            .expect("Failed to load mesh");
        let collider = mesh_to_collider(mesh).expect("Failed to create collider");
        let direction = Quat::from_rotation_z(rng.gen_range(0.0..std::f32::consts::PI * 2.));
        let spawn_distance = Vec3::new(bounds.0.x * 2., bounds.0.y * 2., 0.);
        let translation = direction.mul_vec3(spawn_distance);
        commands.spawn((
            Ufo,
            MaterialMesh2dBundle {
                mesh: ufo_assets.ufo_mesh.clone().into(),
                material: ufo_assets.ufo_material.clone(),
                transform: Transform::from_translation(translation),
                ..default()
            },
            collider,
            CollisionGroups::new(UFO_GROUP, PROJECTILE_GROUP),
            RigidBody::KinematicVelocityBased,
            LockedAxes::ROTATION_LOCKED,
            KillTarget(player_entity),
            TractorBeam::default(),
            ufo_settings.avoidance_weights.clone(),
        ));
        return;
    }
}

#[derive(Component)]
struct InsideBounds;

fn ufo_inside_bounds(
    mut commands: Commands,
    ufo_query: Query<(Entity, &GlobalTransform), With<Ufo>>,
    bounds: Res<Bounds>,
) {
    let bounds_rect = Rect::from_center_half_size(Vec2::ZERO, bounds.0);
    for (ufo_entity, ufo_transform) in ufo_query.iter() {
        if bounds_rect.contains(ufo_transform.translation().xy()) {
            commands
                .entity(ufo_entity)
                .insert((InsideBounds, Duplicable));
        } else {
            commands.entity(ufo_entity).remove::<InsideBounds>();
        }
    }
}

#[derive(Event)]
pub struct UfoDestroyedEvent {
    pub ufo_entity: Entity,
}

fn ufo_destroyed(
    mut commands: Commands,
    mut ufo_destroyed_events: EventReader<UfoDestroyedEvent>,
    mut meshes: ResMut<Assets<Mesh>>,
    ufo_assets: Res<UfoAssets>,
    ufo_query: Query<(&Transform, Option<&Velocity>)>,
    mut spawn_timer: ResMut<SpawnTimer>,
    mut explosion_events: EventWriter<explosion::ExplosionEvent>,
) {
    for UfoDestroyedEvent { ufo_entity } in ufo_destroyed_events.read() {
        let mesh = meshes
            .get(&ufo_assets.ufo_mesh)
            .expect("Failed to load mesh")
            .clone();

        let (ufo_transform, opt_ufo_velocity) = ufo_query.get(*ufo_entity).expect("UFO not found");

        spawn_shattered_mesh(
            &mesh,
            ufo_assets.ufo_material.clone(),
            ufo_transform,
            opt_ufo_velocity.copied().unwrap_or(Velocity::zero()),
            &mut commands,
            &mut meshes,
        );

        info!("UFO destroyed");
        commands.entity(*ufo_entity).despawn_recursive();

        explosion_events.send(explosion::ExplosionEvent {
            position: ufo_transform.translation.xy(),
            radius: 15.,
        });

        spawn_timer.timer.reset();
    }
}
