use bevy::{
    app::{App, Plugin, Update},
    asset::{Assets, Handle},
    ecs::{
        component::Component,
        entity::Entity,
        schedule::{IntoSystemConfigs, SystemSet},
        system::{Commands, Query, Res, ResMut},
    },
    math::{Vec2, Vec3Swizzles},
    render::mesh::Mesh,
    sprite::{ColorMaterial, MaterialMesh2dBundle, Mesh2dHandle},
    time::{Time, Timer, TimerMode},
    transform::components::Transform,
    utils::default,
};
use bevy_rapier2d::{
    dynamics::{RigidBody, Sleeping, Velocity},
    geometry::{CollisionGroups, Group, Restitution},
};
use itertools::Itertools;
use rand::{rngs::ThreadRng, Rng};

use crate::{edge_wrap::Duplicable, split_mesh::shatter_mesh, utils::mesh_to_collider};

pub struct ShatterPlugin;

impl Plugin for ShatterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, debris_lifetime.in_set(ShatterSet));
    }
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct ShatterSet;

#[derive(Component)]
pub struct Debris {
    lifetime: Timer,
}

const DEBRIS_MAX_AREA: f32 = 6.;
const DEBRIS_MAX_ANG_VELOCITY: f32 = 10.;

pub fn spawn_shattered_mesh(
    mesh: &Mesh,
    material_handle: Handle<ColorMaterial>,
    transform: &Transform,
    velocity: Velocity,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
) {
    let mut rng = ThreadRng::default();
    let shards = shatter_mesh(mesh, DEBRIS_MAX_AREA)
        .into_iter()
        .map(|(mesh, offset)| create_shard(transform, offset, velocity, &mut rng, mesh));

    spawn_debris_batch(commands, shards, meshes, material_handle);
}

pub fn spawn_shattered_mesh_batch(
    commands: &mut Commands,
    material_handle: Handle<ColorMaterial>,
    debris: impl Iterator<Item = (Transform, Velocity, Mesh)>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    let mut rng = ThreadRng::default();
    let debris_bundles = debris
        .flat_map(|(transform, velocity, mesh)| {
            shatter_mesh(&mesh, DEBRIS_MAX_AREA)
                .into_iter()
                .map(move |(mesh, offset)| (transform, velocity, mesh, offset))
        })
        .map(move |(transform, velocity, mesh, offset)| {
            create_shard(&transform, offset, velocity, &mut rng, mesh)
        });

    spawn_debris_batch(commands, debris_bundles, meshes, material_handle);
}

fn create_shard(
    origin: &Transform,
    offset: Vec2,
    velocity: Velocity,
    rng: &mut ThreadRng,
    mesh: Mesh,
) -> (Transform, Velocity, Mesh) {
    let shard_translation = origin.transform_point(offset.extend(0.));
    let shard_transform =
        Transform::from_translation(shard_translation).with_rotation(origin.rotation);

    let rng_range_max = 5.;

    let velocity = Velocity {
        linvel: origin.rotation.mul_vec3(offset.extend(0.)).normalize().xy() * 15.
            + velocity.linvel
            + Vec2::new(
                rng.gen_range(-rng_range_max..rng_range_max),
                rng.gen_range(-rng_range_max..rng_range_max),
            ),
        angvel: rng.gen_range(-DEBRIS_MAX_ANG_VELOCITY..DEBRIS_MAX_ANG_VELOCITY),
    };

    (shard_transform, velocity, mesh)
}

pub const DEBRIS_GROUP: Group = Group::GROUP_4;

fn spawn_debris_batch(
    commands: &mut Commands,
    debris: impl Iterator<Item = (Transform, Velocity, Mesh)>,
    meshes: &mut Assets<Mesh>,
    material_handle: Handle<ColorMaterial>,
) {
    let mut rng = ThreadRng::default();
    let debris_bundles = debris
        .map(|(transform, velocity, mesh)| {
            let collider = mesh_to_collider(&mesh).expect("Failed to create collider");
            (
                Debris {
                    lifetime: Timer::from_seconds(rng.gen_range(0.5..5.0), TimerMode::Once),
                },
                MaterialMesh2dBundle {
                    transform,
                    mesh: Mesh2dHandle(meshes.add(mesh)),
                    material: material_handle.clone(),
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
