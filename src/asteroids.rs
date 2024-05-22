use bevy::{
    asset::Assets,
    ecs::{
        component::Component,
        entity::Entity,
        query::With,
        system::{Commands, Query, Res, ResMut},
    },
    hierarchy::DespawnRecursiveExt,
    log::info,
    math::{
        primitives::{Rectangle, RegularPolygon},
        Vec2, Vec3,
    },
    render::{
        color::Color,
        mesh::{Mesh, VertexAttributeValues},
    },
    sprite::{ColorMaterial, MaterialMesh2dBundle},
    transform::components::Transform,
    utils::default,
};
use bevy_rapier2d::{
    dynamics::{RigidBody, Sleeping, Velocity},
    geometry::{ActiveEvents, Restitution},
};
use rand::Rng;

use crate::{
    edge_wrap::{Bounds, Duplicable},
    utils::mesh_to_collider,
};

#[derive(Component)]
pub struct Asteroid;

pub const ASTEROID_SPAWN_COUNT: usize = 20;
pub const ASTEROID_MAX_VERTICE_DRIFT: f32 = 10.;
pub const ASTEROID_MAX_SPAWN_LIN_VELOCITY: f32 = 50.;
pub const ASTEROID_MAX_SPAWN_ANG_VELOCITY: f32 = 1.;

pub const RECTANGULAR_ASTEROIDS: bool = false;
pub const FROZEN_ASTEROIDS: bool = false;

pub fn spawn_asteroids(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    bounds: Res<Bounds>,
) {
    info!(bounds= ?bounds, number= ?ASTEROID_SPAWN_COUNT, "Spawning asteroids");
    let mut asteroid_positions: Vec<Vec2> = Vec::new();
    while asteroid_positions.len() < ASTEROID_SPAWN_COUNT {
        let mut rng = rand::thread_rng();
        let max_x = bounds.0.x;
        let max_y = bounds.0.y;
        let asteroid_pos = Vec2::new(rng.gen_range(-max_x..max_x), rng.gen_range(-max_y..max_y));

        // skip spawning asteroids on top of player
        if asteroid_pos.length() < 150. {
            continue;
        }

        // skip spawning asteroids on top of other asteroids
        if asteroid_positions
            .iter()
            .any(|&pos| (pos - asteroid_pos).length() < 150.)
        {
            continue;
        }

        asteroid_positions.push(asteroid_pos);

        spawn_asteroid(
            rng,
            &mut commands,
            asteroid_pos,
            &mut meshes,
            &mut materials,
        );
    }
}

pub fn spawn_asteroid(
    mut rng: rand::prelude::ThreadRng,
    commands: &mut Commands,
    asteroid_pos: Vec2,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) {
    let asteroid_velocity = Vec2::new(
        rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY),
        rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY),
    );
    let asteroid_angular_velocity =
        rng.gen_range(-ASTEROID_MAX_SPAWN_ANG_VELOCITY..ASTEROID_MAX_SPAWN_ANG_VELOCITY);

    let mut asteroid_mesh = if !RECTANGULAR_ASTEROIDS {
        Mesh::from(RegularPolygon::new(50., 10))
    } else {
        Mesh::from(Rectangle::new(100., 100.))
    };

    if !RECTANGULAR_ASTEROIDS {
        let pos_attributes = asteroid_mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION).expect(
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
    }

    let collider = mesh_to_collider(&asteroid_mesh);

    let mut asteroid_cmd = commands.spawn((
        Asteroid,
        MaterialMesh2dBundle {
            transform: Transform::default().with_translation(Vec3::new(
                asteroid_pos.x,
                asteroid_pos.y,
                0.,
            )),
            mesh: meshes.add(asteroid_mesh).into(),
            material: materials.add(ColorMaterial::from(Color::WHITE)),
            ..default()
        },
        collider,
        ActiveEvents::COLLISION_EVENTS,
        Duplicable,
    ));

    if !FROZEN_ASTEROIDS {
        asteroid_cmd.insert((
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
        ));
    } else {
        asteroid_cmd.insert(RigidBody::Fixed);
    }
}

pub fn despawn_asteroids(
    mut commands: Commands,
    mut asteroid_query: Query<Entity, With<Asteroid>>,
) {
    for asteroid_entity in asteroid_query.iter_mut() {
        commands.entity(asteroid_entity).despawn_recursive();
    }
}
