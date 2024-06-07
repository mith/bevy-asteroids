use bevy::{ecs::component::Component, time::Timer};
use bevy::{prelude::*, sprite::MaterialMesh2dBundle};

pub struct ExplosionPlugin;

impl Plugin for ExplosionPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ExplosionEvent>()
            .add_systems(Startup, load_explosion_assets)
            .add_systems(
                Last,
                (spawn_explosion_event, explosion_expansion).in_set(ExplosionSet),
            );
    }
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct ExplosionSet;

const EXPLOSION_DURATION: f32 = 0.25;

#[derive(Component)]
pub struct Explosion {
    pub lifetime: Timer,
}

impl Default for Explosion {
    fn default() -> Self {
        Self {
            lifetime: Timer::from_seconds(EXPLOSION_DURATION, TimerMode::Once),
        }
    }
}

#[derive(Resource)]
struct ExplosionAssets {
    explosion_sound: Handle<AudioSource>,
}

fn load_explosion_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(ExplosionAssets {
        explosion_sound: asset_server.load("audio/explosion.mp3"),
    });
}

#[derive(Event)]
pub struct ExplosionEvent {
    pub position: Vec2,
    pub radius: f32,
}

fn spawn_explosion_event(
    mut commands: Commands,
    mut events: EventReader<ExplosionEvent>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    explosion_assets: Res<ExplosionAssets>,
) {
    for event in events.read() {
        spawn_explosion(
            &mut commands,
            &mut meshes,
            &mut materials,
            &explosion_assets,
            &Transform::from_translation(event.position.extend(0.)),
            event.radius,
        );
    }
}

#[derive(Component)]
pub struct ExplosionSound;

pub fn spawn_explosion(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    explosion_assets: &ExplosionAssets,
    transform: &Transform,
    radius: f32,
) {
    commands.spawn((
        Explosion::default(),
        MaterialMesh2dBundle {
            transform: *transform,
            mesh: meshes.add(Circle::new(radius)).into(),
            material: materials.add(ColorMaterial::from(Color::RED)),
            ..default()
        },
    ));
    commands.spawn((
        ExplosionSound,
        AudioBundle {
            source: explosion_assets.explosion_sound.clone(),
            settings: PlaybackSettings::DESPAWN,
        },
    ));
}

fn explosion_expansion(
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &mut Explosion,
        &mut Transform,
        &mut Handle<ColorMaterial>,
    )>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    time: Res<Time>,
) {
    for (entity, mut explosion, mut transform, material_handle) in query.iter_mut() {
        if explosion.lifetime.tick(time.delta()).just_finished() {
            commands.entity(entity).despawn();
        } else {
            let material = materials.get_mut(material_handle.id()).unwrap();
            material
                .color
                .set_a(explosion.lifetime.fraction_remaining());

            transform.scale *= 1.04;
        }
    }
}
