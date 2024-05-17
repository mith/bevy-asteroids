use bevy::{
    asset::Assets,
    core::Name,
    ecs::{
        component::Component,
        entity::Entity,
        query::With,
        system::{Commands, Query, ResMut},
    },
    hierarchy::{BuildChildren, DespawnRecursiveExt},
    math::{primitives::RegularPolygon, Vec3},
    render::{color::Color, mesh::Mesh, view::Visibility},
    sprite::{ColorMaterial, MaterialMesh2dBundle},
    transform::components::Transform,
    utils::default,
};
use bevy_rapier2d::dynamics::RigidBody;

use crate::{
    edge_wrap::Duplicable,
    ship::{Ship, Thruster},
    utils::mesh_to_collider,
};

#[derive(Component)]
pub struct Player;

pub fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    spawn_player_ship(
        &mut commands,
        &mut meshes,
        &mut materials,
        Transform::default(),
    );
}

fn spawn_player_ship(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    transform: Transform,
) {
    let player_shape = RegularPolygon::new(10., 3);

    let player_mesh = Mesh::from(player_shape);
    let collider = mesh_to_collider(&player_mesh);
    commands
        .spawn((
            Name::new("Player"),
            Player,
            Ship,
            MaterialMesh2dBundle {
                mesh: meshes.add(player_mesh).into(),
                material: materials.add(ColorMaterial::from(Color::WHITE)),
                transform,
                ..default()
            },
            RigidBody::Dynamic,
            collider,
            Duplicable,
        ))
        .with_children(|parent| {
            parent.spawn((
                Name::new("Thruster"),
                Thruster,
                MaterialMesh2dBundle {
                    transform: Transform::from_translation(Vec3::new(0., -10., 0.)),
                    mesh: meshes.add(Mesh::from(RegularPolygon::new(5., 3))).into(),
                    material: materials.add(ColorMaterial::from(Color::RED)),
                    visibility: Visibility::Hidden,
                    ..default()
                },
            ));
        });
}

pub fn despawn_player(mut commands: Commands, mut player_query: Query<Entity, With<Player>>) {
    for player_entity in player_query.iter_mut() {
        commands.entity(player_entity).despawn_recursive();
    }
}
