use bevy::{
    app::{App, Plugin, Startup, Update},
    asset::{AssetServer, Assets, Handle},
    audio::{AudioBundle, AudioSource, PlaybackSettings},
    core::Name,
    ecs::{
        component::Component,
        entity::Entity,
        event::{Event, EventReader},
        schedule::{IntoSystemConfigs, SystemSet},
        system::{Commands, Query, Res, ResMut, Resource},
    },
    math::{Vec3, Vec3Swizzles},
    render::mesh::Mesh,
    sprite::ColorMaterial,
    time::{Time, Timer, TimerMode},
    transform::components::Transform,
};

use crate::projectile::spawn_projectile;

pub struct TurretPlugin;

impl Plugin for TurretPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<FireEvent>()
            .add_systems(Startup, load_turret_assets)
            .add_systems(Update, (reload, fire_projectile).chain().in_set(TurretSet));
    }
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct TurretSet;

#[derive(Event, Debug, Clone, Copy)]
pub struct FireEvent {
    pub turret_entity: Entity,
}

#[derive(Component)]
pub struct ReloadTimer(Timer);

const RELOAD_DURATION: f32 = 0.3;

impl Default for ReloadTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(RELOAD_DURATION, TimerMode::Once))
    }
}

pub fn reload(
    mut commands: Commands,
    mut reload_timer_query: Query<(Entity, &mut ReloadTimer)>,
    time: Res<Time>,
) {
    for (entity, mut reload_timer) in reload_timer_query.iter_mut() {
        if reload_timer.0.tick(time.delta()).just_finished() {
            commands.entity(entity).remove::<ReloadTimer>();
        }
    }
}

#[derive(Resource)]
struct TurretAssets {
    firing_sound: Handle<AudioSource>,
}

#[derive(Component)]
struct TurretFireSound;

fn load_turret_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(TurretAssets {
        firing_sound: asset_server.load("audio/turret_fire.mp3"),
    });
}

fn fire_projectile(
    mut commands: Commands,
    mut fire_event_reader: EventReader<FireEvent>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    transform_query: Query<&Transform>,
    reload_timer_query: Query<&ReloadTimer>,
    turret_assets: Res<TurretAssets>,
) {
    for FireEvent { turret_entity } in fire_event_reader.read() {
        if reload_timer_query.contains(*turret_entity) {
            continue;
        }
        if let Some(ref mut turret_cmd) = commands.get_entity(*turret_entity) {
            turret_cmd.insert(ReloadTimer::default());
        } else {
            continue;
        }
        let turret_transform = transform_query.get(*turret_entity).unwrap();

        let position = turret_transform.translation.xy()
            + turret_transform
                .rotation
                .mul_vec3(Vec3::new(0., 10., 0.))
                .xy();

        let velocity = turret_transform
            .rotation
            .mul_vec3(Vec3::new(0., 1., 0.))
            .xy()
            * 500.;
        spawn_projectile(
            &mut commands,
            &mut meshes,
            &mut materials,
            position,
            velocity,
        );
        commands.spawn((
            Name::from("Turret fire sound"),
            TurretFireSound,
            AudioBundle {
                source: turret_assets.firing_sound.clone(),
                settings: PlaybackSettings::DESPAWN,
            },
        ));
    }
}
