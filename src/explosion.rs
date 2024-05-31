use bevy::{ecs::component::Component, time::Timer};
use bevy::{prelude::*, sprite::MaterialMesh2dBundle};

pub struct ExplosionPlugin;

impl Plugin for ExplosionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, explosion_expansion.in_set(ExplosionSet));
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

pub fn spawn_explosion(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
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
