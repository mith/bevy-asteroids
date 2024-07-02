use crate::{
    asteroid::{Asteroid, SplitAsteroidEvent, ASTEROID_GROUP},
    edge_wrap::{get_original_entities, Duplicable, Duplicate},
    explosion::ExplosionEvent,
    ufo::{Ufo, UfoDestroyedEvent, UFO_GROUP},
    utils::{contact_position_and_normal, mesh_to_collider},
};
use bevy::{ecs::component::Component, time::Timer};
use bevy::{prelude::*, sprite::MaterialMesh2dBundle};
use bevy_rapier2d::{
    dynamics::{RigidBody, Velocity},
    geometry::{ActiveEvents, CollisionGroups, Group},
    plugin::RapierContext,
    prelude::CollisionEvent,
};

pub struct ProjectilePlugin;

impl Plugin for ProjectilePlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ProjectileExplosionEvent>().add_systems(
            Update,
            (
                projectile_timer,
                (projectile_asteroid_collision, projectile_ufo_collision),
                projectile_explosion,
            )
                .chain()
                .after(ProjectileSet),
        );
    }
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct ProjectileSet;

#[derive(Component)]
pub struct Projectile {
    pub lifetime: Timer,
}

pub const PROJECTILE_GROUP: Group = Group::GROUP_2;
pub const PROJECTILE_LIFETIME: f32 = 5.;
pub const PROJECTILE_RADIUS: f32 = 4.;

pub fn spawn_projectile(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    position: Vec2,
    velocity: Vec2,
) {
    let projectile_shape = Circle::new(PROJECTILE_RADIUS);

    let projectile_mesh = Mesh::from(projectile_shape);
    let collider = mesh_to_collider(&projectile_mesh).expect("Failed to create collider");
    commands.spawn((
        Projectile {
            lifetime: Timer::from_seconds(PROJECTILE_LIFETIME, TimerMode::Once),
        },
        MaterialMesh2dBundle {
            mesh: meshes.add(projectile_mesh).into(),
            material: materials.add(ColorMaterial::from(Color::WHITE)),
            transform: Transform::from_translation(position.extend(0.)),
            ..default()
        },
        RigidBody::Dynamic,
        Velocity {
            linvel: velocity,
            ..default()
        },
        collider,
        Duplicable,
        ActiveEvents::COLLISION_EVENTS,
        CollisionGroups::new(PROJECTILE_GROUP, ASTEROID_GROUP | UFO_GROUP),
    ));
}

fn projectile_timer(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Projectile)>,
    time: Res<Time>,
) {
    for (entity, mut projectile) in query.iter_mut() {
        if projectile.lifetime.tick(time.delta()).just_finished() {
            info!("Projectile expired");
            commands.entity(entity).despawn();
        }
    }
}

#[derive(Event, Debug, Clone, Copy)]
pub struct ProjectileExplosionEvent {
    pub projectile_entity: Entity,
}

fn projectile_asteroid_collision(
    rapier_context: Res<RapierContext>,
    mut collision_events: EventReader<CollisionEvent>,
    projectile_query: Query<&Projectile>,
    mut asteroid_query: Query<(&Transform, Option<&Velocity>), With<Asteroid>>,
    duplicate_query: Query<&Duplicate>,
    transform_query: Query<&GlobalTransform>,
    mut split_asteroid_events: EventWriter<SplitAsteroidEvent>,
    mut projectile_explosion_events: EventWriter<ProjectileExplosionEvent>,
) {
    for event in collision_events.read() {
        if let CollisionEvent::Started(entity_a, entity_b, _) = event {
            let (entity_a, duplicate_a) = get_original_entities(&duplicate_query, entity_a);
            let (entity_b, duplicate_b) = get_original_entities(&duplicate_query, entity_b);

            let ((projectile_entity, _projectile_duplicate), (asteroid_entity, asteroid_duplicate)) =
                if projectile_query.contains(entity_a) && asteroid_query.contains(entity_b) {
                    ((entity_a, duplicate_a), (entity_b, duplicate_b))
                } else if projectile_query.contains(entity_b) && asteroid_query.contains(entity_a) {
                    ((entity_b, duplicate_b), (entity_a, duplicate_a))
                } else {
                    continue;
                };

            projectile_explosion_events.send(ProjectileExplosionEvent { projectile_entity });

            // Split asteroid into smaller asteroids
            let (transform, velocity) = asteroid_query
                .get_mut(asteroid_entity)
                .expect("Asteroid not found");
            let projectile_transform = transform_query
                .get(projectile_entity)
                .expect("Projectile transform not found");

            let Some((collision_position, collision_direction)) = contact_position_and_normal(
                &rapier_context,
                projectile_entity,
                asteroid_duplicate.unwrap_or(asteroid_entity),
            ) else {
                continue;
            };

            let mut velocity = velocity.copied().unwrap_or_else(Velocity::zero);
            velocity.linvel -=
                (projectile_transform.translation().xy() - transform.translation.xy()).normalize()
                    * 100.;

            split_asteroid_events.send(SplitAsteroidEvent {
                asteroid_entity,
                collision_direction,
                collision_position,
            });
        }
    }
}

fn projectile_ufo_collision(
    mut collision_events: EventReader<CollisionEvent>,
    projectile_query: Query<&Projectile>,
    ufo_query: Query<Entity, With<Ufo>>,
    mut ufo_destroyed_events: EventWriter<UfoDestroyedEvent>,
    mut projectile_explosion_events: EventWriter<ProjectileExplosionEvent>,
) {
    for event in collision_events.read() {
        if let CollisionEvent::Started(entity_a, entity_b, _) = event {
            let (projectile_entity, ufo_entity) =
                if projectile_query.contains(*entity_a) && ufo_query.contains(*entity_b) {
                    (entity_a, entity_b)
                } else if projectile_query.contains(*entity_b) && ufo_query.contains(*entity_a) {
                    (entity_b, entity_a)
                } else {
                    continue;
                };

            ufo_destroyed_events.send(UfoDestroyedEvent {
                ufo_entity: *ufo_entity,
            });

            projectile_explosion_events.send(ProjectileExplosionEvent {
                projectile_entity: *projectile_entity,
            });
        }
    }
}

fn projectile_explosion(
    mut commands: Commands,
    mut events: EventReader<ProjectileExplosionEvent>,
    mut explosion_events: EventWriter<ExplosionEvent>,
    transform_query: Query<&Transform>,
) {
    for event in events.read() {
        let transform = transform_query
            .get(event.projectile_entity)
            .expect("Projectile transform not found");
        explosion_events.send(ExplosionEvent {
            position: transform.translation.xy(),
            radius: PROJECTILE_RADIUS,
        });
        info!("Projectile exploded");
        commands.entity(event.projectile_entity).despawn_recursive();
    }
}
