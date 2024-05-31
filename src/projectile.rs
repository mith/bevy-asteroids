use crate::{
    asteroid::{Asteroid, SplitAsteroidEvent, ASTEROID_GROUP},
    edge_wrap::{Duplicable, Duplicate},
    utils::mesh_to_collider,
};
use bevy::{ecs::component::Component, time::Timer};
use bevy::{prelude::*, sprite::MaterialMesh2dBundle};
use bevy_rapier2d::{
    dynamics::{RigidBody, Velocity},
    geometry::{CollisionGroups, Group},
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
                apply_deferred,
                projectile_asteroid_collision,
                projectile_explosion,
                explosion_expansion,
                apply_deferred,
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
    let collider = mesh_to_collider(&projectile_mesh);
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
        CollisionGroups::new(PROJECTILE_GROUP, ASTEROID_GROUP),
    ));
}

fn projectile_timer(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Projectile)>,
    time: Res<Time>,
) {
    for (entity, mut projectile) in query.iter_mut() {
        if projectile.lifetime.tick(time.delta()).just_finished() {
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
            let contact = rapier_context
                .contact_pair(
                    projectile_entity,
                    asteroid_duplicate.unwrap_or(asteroid_entity),
                )
                .expect("No contact found for projectile-asteroid collision");
            if !contact.has_any_active_contacts() {
                continue;
            }
            let (contact_manifold, contact) = contact
                .find_deepest_contact()
                .expect("No contact point found for projectile-asteroid collision");

            let mut velocity = velocity.copied().unwrap_or_else(Velocity::zero);
            velocity.linvel -=
                (projectile_transform.translation().xy() - transform.translation.xy()).normalize()
                    * 100.;

            split_asteroid_events.send(SplitAsteroidEvent {
                asteroid_entity,
                collision_direction: contact_manifold.normal(),
                collision_position: contact.local_p2(),
            });
        }
    }
}

fn get_original_entities(
    duplicate_query: &Query<&Duplicate, ()>,
    entity_a: &Entity,
) -> (Entity, Option<Entity>) {
    if let Ok(Duplicate { original }) = duplicate_query.get(*entity_a) {
        (*original, Some(*entity_a))
    } else {
        (*entity_a, None)
    }
}

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

fn projectile_explosion(
    mut commands: Commands,
    mut events: EventReader<ProjectileExplosionEvent>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    transform_query: Query<&Transform>,
) {
    for event in events.read() {
        let transform = transform_query
            .get(event.projectile_entity)
            .expect("Projectile transform not found");
        spawn_explosion(&mut commands, &mut meshes, &mut materials, transform);
        commands.entity(event.projectile_entity).despawn_recursive();
    }
}

fn spawn_explosion(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    transform: &Transform,
) {
    commands.spawn((
        Explosion::default(),
        MaterialMesh2dBundle {
            transform: *transform,
            mesh: meshes.add(Circle::new(PROJECTILE_RADIUS)).into(),
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
