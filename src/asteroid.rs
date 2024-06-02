use bevy::{
    app::{App, Plugin, Update},
    asset::{Assets, Handle},
    ecs::{
        bundle::Bundle,
        component::Component,
        entity::Entity,
        event::{Event, EventReader},
        schedule::{IntoSystemConfigs, SystemSet},
        system::{Command, Commands, EntityCommand, EntityCommands, Query, Res, ResMut},
        world::Mut,
    },
    log::{error, info},
    math::{primitives::RegularPolygon, Vec2, Vec3, Vec3Swizzles},
    prelude::World,
    render::{
        color::Color,
        mesh::{Mesh, VertexAttributeValues},
    },
    sprite::{ColorMaterial, MaterialMesh2dBundle, Mesh2dHandle},
    time::{Time, Timer, TimerMode},
    transform::components::Transform,
    utils::default,
};
use bevy_rapier2d::{
    dynamics::{RigidBody, Sleeping, Velocity},
    geometry::{ActiveEvents, CollisionGroups, Group, Restitution},
};
use itertools::Itertools;
use rand::{rngs::ThreadRng, Rng};

use crate::{
    edge_wrap::{Bounds, Duplicable},
    mesh_utils::calculate_mesh_area,
    split_mesh::{shatter_mesh, split_mesh, trim_mesh},
    utils::mesh_to_collider,
};

pub struct AsteroidPlugin;

impl Plugin for AsteroidPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<SplitAsteroidEvent>().add_systems(
            Update,
            (split_asteroid_event, debris_lifetime)
                .chain()
                .in_set(AsteroidSet),
        );
    }
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct AsteroidSet;

#[derive(Component)]
pub struct Asteroid;

const ASTEROID_MAX_VERTICES: usize = 14;
const ASTEROID_MAX_VERTICE_DRIFT: f32 = 8.;
const ASTEROID_MAX_SPAWN_LIN_VELOCITY: f32 = 50.;
const ASTEROID_MAX_SPAWN_ANG_VELOCITY: f32 = 1.;
const ASTEROID_SPAWN_CIRCUMRADIUS: f32 = 50.;

pub const ASTEROID_GROUP: Group = Group::GROUP_3;

pub fn spawn_asteroids(mut commands: Commands, bounds: Res<Bounds>) {
    // Divide bounds area by approximate asteroid area to get a rough estimate of how many asteroids to spawn
    let asteroid_spawn_count = (((bounds.0.x * bounds.0.y) as usize
        / (ASTEROID_SPAWN_CIRCUMRADIUS * ASTEROID_SPAWN_CIRCUMRADIUS) as usize)
        / 10)
        .clamp(2, 10);
    info!(bounds= ?bounds, number= ?asteroid_spawn_count, "Spawning asteroids");
    let mut rng = rand::thread_rng();
    let asteroid_positions: Vec<Vec2> = (0..asteroid_spawn_count)
        .map(|_| {
            Vec2::new(
                rng.gen_range(-bounds.0.x..bounds.0.x),
                rng.gen_range(-bounds.0.y..bounds.0.y),
            )
        })
        .filter(|position| position.length() > 150.)
        .fold(Vec::new(), |mut acc, position| {
            if acc.iter().all(|&pos| (pos - position).length() > 150.) {
                acc.push(position);
            }
            acc
        });
    commands.spawn_asteroid_batch(asteroid_positions);
}

struct SpawnAsteroid {
    position: Vec2,
}

impl EntityCommand for SpawnAsteroid {
    fn apply(self, entity: Entity, world: &mut World) {
        let mut rng = ThreadRng::default();
        let asteroid_pos = self.position;
        let asteroid_velocity = Vec2::new(
            rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY),
            rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY),
        );
        let asteroid_angular_velocity =
            rng.gen_range(-ASTEROID_MAX_SPAWN_ANG_VELOCITY..ASTEROID_MAX_SPAWN_ANG_VELOCITY);

        let (asteroid_mesh_handle, collider) = world.resource_scope(|_world, mut meshes: Mut<Assets<Mesh>>| {
            let mut mesh = Mesh::from(RegularPolygon::new(
                ASTEROID_SPAWN_CIRCUMRADIUS,
                ASTEROID_MAX_VERTICES,
            ));

            let pos_attributes = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION).expect(
            "Mesh does not have a position attribute. This should not happen as we just created the mesh",
            );

            let VertexAttributeValues::Float32x3(pos_attr_vec3) = pos_attributes else {
                panic!("Position attribute is not a Float32x3");
            };

            pos_attr_vec3.iter_mut().for_each(|v| {
                // Translate vertice randomly
                v[0] += rng.gen_range(-ASTEROID_MAX_VERTICE_DRIFT..ASTEROID_MAX_VERTICE_DRIFT);
                v[1] += rng.gen_range(-ASTEROID_MAX_VERTICE_DRIFT..ASTEROID_MAX_VERTICE_DRIFT);
            });

            let collider = mesh_to_collider(&mesh);
            (meshes.add(mesh), collider)
        });

        let material_handle =
            world.resource_scope(|_world, mut materials: Mut<Assets<ColorMaterial>>| {
                materials.add(ColorMaterial::from(Color::WHITE))
            });

        world.entity_mut(entity).insert(create_asteroid_bundle(
            asteroid_pos,
            asteroid_mesh_handle,
            material_handle,
            collider,
            asteroid_velocity,
            asteroid_angular_velocity,
        ));
    }
}

fn create_asteroid_bundle(
    asteroid_pos: Vec2,
    asteroid_mesh_handle: Handle<Mesh>,
    material_handle: Handle<ColorMaterial>,
    collider: bevy_rapier2d::prelude::Collider,
    asteroid_velocity: Vec2,
    asteroid_angular_velocity: f32,
) -> impl Bundle {
    (
        Asteroid,
        MaterialMesh2dBundle {
            transform: Transform::default().with_translation(Vec3::new(
                asteroid_pos.x,
                asteroid_pos.y,
                0.,
            )),
            mesh: asteroid_mesh_handle.into(),
            material: material_handle,
            ..default()
        },
        collider,
        ActiveEvents::COLLISION_EVENTS,
        Duplicable,
        CollisionGroups::new(ASTEROID_GROUP, Group::ALL),
        RigidBody::Dynamic,
        Velocity {
            linvel: asteroid_velocity,
            angvel: asteroid_angular_velocity,
        },
        Restitution {
            coefficient: 0.9,
            ..default()
        },
        Sleeping {
            normalized_linear_threshold: 0.001,
            angular_threshold: 0.001,
            ..default()
        },
    )
}

struct SpawnAsteroidBatch {
    positions: Vec<Vec2>,
}

impl Command for SpawnAsteroidBatch {
    fn apply(self, world: &mut World) {
        let mut rng = ThreadRng::default();
        let asteroid_bundles = self.positions.iter().map(|position|{
            let velocity = Vec2::new(
                rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY),
                rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY),
            );

            let angular_velocity = rng.gen_range(-ASTEROID_MAX_SPAWN_ANG_VELOCITY..ASTEROID_MAX_SPAWN_ANG_VELOCITY);

            let (asteroid_mesh_handle, collider) = world.resource_scope(|_world, mut meshes: Mut<Assets<Mesh>>| {
                let mut mesh = Mesh::from(RegularPolygon::new(
                    ASTEROID_SPAWN_CIRCUMRADIUS,
                    ASTEROID_MAX_VERTICES,
                ));

                let pos_attributes = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION).expect(
                "Mesh does not have a position attribute. This should not happen as we just created the mesh",
                );

                let VertexAttributeValues::Float32x3(pos_attr_vec3) = pos_attributes else {
                    panic!("Position attribute is not a Float32x3");
                };

                pos_attr_vec3.iter_mut().for_each(|v| {
                    // Translate vertice randomly
                    v[0] += rng.gen_range(-ASTEROID_MAX_VERTICE_DRIFT..ASTEROID_MAX_VERTICE_DRIFT);
                    v[1] += rng.gen_range(-ASTEROID_MAX_VERTICE_DRIFT..ASTEROID_MAX_VERTICE_DRIFT);
                });

                let collider = mesh_to_collider(&mesh);
                (meshes.add(mesh), collider)
            });

            let material_handle =
                world.resource_scope(|_world, mut materials: Mut<Assets<ColorMaterial>>| {
                    materials.add(ColorMaterial::from(Color::WHITE))
                });

            create_asteroid_bundle(*position, asteroid_mesh_handle, material_handle, collider, velocity, angular_velocity)
        }).collect_vec();

        world.spawn_batch(asteroid_bundles);
    }
}

pub trait AsteroidSpawnParamExt {
    fn spawn_asteroid(&mut self, position: Vec2) -> EntityCommands;

    fn spawn_asteroid_batch(&mut self, positions: Vec<Vec2>);
}

impl<'w, 's> AsteroidSpawnParamExt for Commands<'w, 's> {
    fn spawn_asteroid(&mut self, position: Vec2) -> EntityCommands {
        let mut e = self.spawn_empty();
        e.add(SpawnAsteroid { position });
        e
    }

    fn spawn_asteroid_batch(&mut self, positions: Vec<Vec2>) {
        self.add(SpawnAsteroidBatch { positions });
    }
}

#[derive(Event)]
pub struct SplitAsteroidEvent {
    pub asteroid_entity: Entity,
    pub collision_direction: Vec2,
    pub collision_position: Vec2,
}

#[derive(Component)]
pub struct Debris {
    lifetime: Timer,
}

const ASTEROID_MIN_AREA: f32 = 500.;

const DEBRIS_MAX_AREA: f32 = 6.;

fn split_asteroid_event(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut asteroid_query: Query<(&Transform, &Velocity, &mut Mesh2dHandle)>,
    mut split_asteroid_events: EventReader<SplitAsteroidEvent>,
) {
    for event in split_asteroid_events.read() {
        let (transform, velocity, mesh_handle) = asteroid_query
            .get_mut(event.asteroid_entity)
            .expect("Asteroid entity not found");
        split_asteroid(
            &mut commands,
            &mesh_handle.0,
            &mut meshes,
            &mut materials,
            transform,
            *velocity,
            event.collision_direction,
            event.collision_position,
        );

        commands.entity(event.asteroid_entity).despawn();
    }
}

fn split_asteroid(
    commands: &mut Commands,
    original_mesh: &Handle<Mesh>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    transform: &Transform,
    velocity: Velocity,
    collision_direction: Vec2,
    collision_position: Vec2,
) {
    let mesh = meshes.get(original_mesh).expect("Original mesh not found");

    // Rotate the collision direction by the rotation of the asteroid
    // to get the collision direction in the asteroid's local space.
    let asteroid_rotation = transform.rotation;
    let mesh_collision_direction = asteroid_rotation
        .inverse()
        .mul_vec3(collision_direction.extend(0.))
        .truncate()
        .normalize();

    let [(mesh_a, offset_a), (mesh_b, offset_b)] =
        split_mesh(mesh, mesh_collision_direction, collision_position);

    // Calculate area of the split mesh

    let mut debris = Vec::new();

    let mut spawn = |mesh: &Mesh, offset: Vec2| {
        // Check if mesh contains any vertices
        if mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .map(|attr| attr.len() < 3)
            .unwrap_or(true)
        {
            error!("Mesh has no triangles, skipping split");
            return;
        }
        let (main_mesh, trimmings) = trim_mesh(mesh);
        let translation = transform.transform_point((offset + main_mesh.1).extend(0.));
        let main_transform =
            Transform::from_translation(translation).with_rotation(transform.rotation);
        let velocity = Velocity {
            linvel: velocity.linvel
                + asteroid_rotation
                    .mul_vec3(offset.extend(0.))
                    .truncate()
                    .normalize()
                    * 50.,
            angvel: velocity.angvel,
        };
        let mesh_area = calculate_mesh_area(mesh);
        if mesh_area > ASTEROID_MIN_AREA {
            // let mesh = round_mesh(mesh).0;
            spawn_asteroid_split(
                commands,
                main_transform,
                velocity,
                meshes,
                materials,
                &main_mesh.0,
            );
        } else if mesh_area > 0. && mesh_area < ASTEROID_MIN_AREA {
            debris.push((main_transform, velocity, main_mesh.0))
        }

        debris.extend(trimmings.into_iter().map(|(mesh, trimmed_offset)| {
            let translation = transform.transform_point((offset + trimmed_offset).extend(0.));
            let transform =
                Transform::from_translation(translation).with_rotation(main_transform.rotation);
            (transform, velocity, mesh)
        }));
    };

    spawn(&mesh_a, offset_a);
    spawn(&mesh_b, offset_b);

    spawn_shattered_mesh_batch(commands, debris.into_iter(), meshes, materials);
}

pub fn spawn_shattered_mesh(
    mesh: &Mesh,
    transform: &Transform,
    velocity: Velocity,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) {
    let mut rng = ThreadRng::default();
    let shards = shatter_mesh(mesh, DEBRIS_MAX_AREA)
        .into_iter()
        .map(|(mesh, offset)| {
            let shard_translation = transform.transform_point(offset.extend(0.));
            let shard_transform =
                Transform::from_translation(shard_translation).with_rotation(transform.rotation);

            let rng_range_max = 5.;

            let velocity = Velocity {
                linvel: transform
                    .rotation
                    .mul_vec3(offset.extend(0.))
                    .normalize()
                    .xy()
                    * 15.
                    + velocity.linvel
                    + Vec2::new(
                        rng.gen_range(-rng_range_max..rng_range_max),
                        rng.gen_range(-rng_range_max..rng_range_max),
                    ),
                angvel: rng
                    .gen_range(-ASTEROID_MAX_SPAWN_ANG_VELOCITY..ASTEROID_MAX_SPAWN_ANG_VELOCITY),
            };

            (shard_transform, velocity, mesh)
        });

    spawn_debris_batch(commands, shards, meshes, materials);
}

pub fn spawn_shattered_mesh_batch(
    commands: &mut Commands,
    debris: impl Iterator<Item = (Transform, Velocity, Mesh)>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) {
    let mut rng = ThreadRng::default();
    let debris_bundles = debris
        .flat_map(|(transform, velocity, mesh)| {
            shatter_mesh(&mesh, DEBRIS_MAX_AREA)
                .into_iter()
                .map(move |(mesh, offset)| (transform, velocity, mesh, offset))
        })
        .map(move |(transform, velocity, mesh, offset)| {
            let rng_range_max = 5.;

            let shard_translation = transform.transform_point(offset.extend(0.));
            let shard_transform =
                Transform::from_translation(shard_translation).with_rotation(transform.rotation);
            let velocity = Velocity {
                linvel: transform
                    .rotation
                    .mul_vec3(offset.extend(0.))
                    .normalize()
                    .xy()
                    * 15.
                    + velocity.linvel
                    + Vec2::new(
                        rng.gen_range(-rng_range_max..rng_range_max),
                        rng.gen_range(-rng_range_max..rng_range_max),
                    ),
                angvel: rng
                    .gen_range(-ASTEROID_MAX_SPAWN_ANG_VELOCITY..ASTEROID_MAX_SPAWN_ANG_VELOCITY),
            };

            (shard_transform, velocity, mesh)
        });

    spawn_debris_batch(commands, debris_bundles, meshes, materials);
}

fn spawn_asteroid_split(
    commands: &mut Commands,
    transform: Transform,
    velocity: Velocity,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    mesh: &Mesh,
) {
    let collider = mesh_to_collider(mesh);

    commands.spawn((
        Asteroid,
        MaterialMesh2dBundle {
            transform,
            mesh: Mesh2dHandle(meshes.add(mesh.clone())),
            material: materials.add(ColorMaterial::from(Color::WHITE)),
            ..default()
        },
        collider,
        ActiveEvents::COLLISION_EVENTS,
        Duplicable,
        RigidBody::Dynamic,
        velocity,
        Restitution {
            coefficient: 0.9,
            ..default()
        },
        Sleeping {
            normalized_linear_threshold: 0.001,
            angular_threshold: 0.001,
            ..default()
        },
    ));
}

pub const DEBRIS_GROUP: Group = Group::GROUP_4;

fn spawn_debris_batch(
    commands: &mut Commands,
    debris: impl Iterator<Item = (Transform, Velocity, Mesh)>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) {
    let mut rng = ThreadRng::default();
    let material = materials.add(ColorMaterial::from(Color::WHITE));
    let debris_bundles = debris
        .map(|(transform, velocity, mesh)| {
            let collider = mesh_to_collider(&mesh);
            (
                Debris {
                    lifetime: Timer::from_seconds(rng.gen_range(0.5..5.0), TimerMode::Once),
                },
                MaterialMesh2dBundle {
                    transform,
                    mesh: Mesh2dHandle(meshes.add(mesh)),
                    material: material.clone(),
                    ..default()
                },
                collider,
                CollisionGroups::new(DEBRIS_GROUP, Group::NONE),
                Duplicable,
                RigidBody::Dynamic,
                velocity,
                Restitution {
                    coefficient: 0.9,
                    ..default()
                },
                Sleeping {
                    normalized_linear_threshold: 0.001,
                    angular_threshold: 0.001,
                    ..default()
                },
            )
        })
        .collect_vec();

    commands.spawn_batch(debris_bundles);
}

fn debris_lifetime(
    mut commands: Commands,
    time: Res<Time>,
    mut debris_query: Query<(Entity, &mut Debris)>,
) {
    for (entity, mut debris) in &mut debris_query {
        debris.lifetime.tick(time.delta());
        if debris.lifetime.finished() {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {

    use bevy::{
        app::{App, Startup},
        math::{primitives::Rectangle, Quat},
    };

    use crate::asteroid::split_asteroid;

    use super::*;

    #[test]
    fn test_split_asteroid_rectangle() {
        let mut app = App::new();

        app.insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<ColorMaterial>::default());

        app.add_systems(
            Startup,
            |mut commands: Commands,
             mut meshes: ResMut<Assets<Mesh>>,
             mut materials: ResMut<Assets<ColorMaterial>>| {
                let rectangle_shape = Rectangle::from_size(Vec2::new(100., 100.));
                let asteroid_mesh = Mesh::from(rectangle_shape);
                let mesh_handle = meshes.add(asteroid_mesh.clone());

                let transform = Transform::default();

                split_asteroid(
                    &mut commands,
                    &mesh_handle,
                    &mut meshes,
                    &mut materials,
                    &transform,
                    Velocity::zero(),
                    Vec2::new(0., 1.),
                    Vec2::ZERO,
                );
            },
        );

        app.run();

        // Check that 2 splits were created
        // They should be located at (-25, 0) and (25, 0)

        assert_eq!(app.world.query::<&Asteroid>().iter(&app.world).len(), 2);

        app.world
            .query::<(&Transform, &Asteroid)>()
            .iter(&app.world)
            .for_each(|(transform, _)| {
                let translation = transform.translation;
                assert!(translation.x == -25. || translation.x == 25.);
                assert_eq!(translation.y, 0.);
            });
    }

    #[test]
    fn test_split_asteroid_rectangle_90_cw_rotated() {
        let mut app = App::new();

        app.insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<ColorMaterial>::default());

        app.add_systems(
            Startup,
            |mut commands: Commands,
             mut meshes: ResMut<Assets<Mesh>>,
             mut materials: ResMut<Assets<ColorMaterial>>| {
                let rectangle_shape = Rectangle::from_size(Vec2::new(100., 100.));
                let asteroid_mesh = Mesh::from(rectangle_shape);
                let mesh_handle = meshes.add(asteroid_mesh.clone());

                let transform =
                    Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));

                split_asteroid(
                    &mut commands,
                    &mesh_handle,
                    &mut meshes,
                    &mut materials,
                    &transform,
                    Velocity::zero(),
                    Vec2::new(0., 1.),
                    Vec2::ZERO,
                );
            },
        );

        app.run();

        // Check that 2 splits were created
        // They should be located at (0, -25) and (0, 25)

        assert_eq!(app.world.query::<&Asteroid>().iter(&app.world).len(), 2);

        app.world
            .query::<(&Transform, &Asteroid)>()
            .iter(&app.world)
            .for_each(|(transform, _)| {
                let translation = transform.translation;
                assert!(translation.y == -25. || translation.y == 25.);
                assert_eq!(translation.x, 0.);
            });
    }
}
