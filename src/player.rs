use bevy::{
    asset::Assets,
    core::Name,
    ecs::{
        component::Component,
        system::{Commands, ResMut},
    },
    hierarchy::BuildChildren,
    math::{
        primitives::{RegularPolygon, Triangle2d},
        Vec2, Vec3,
    },
    render::{color::Color, mesh::Mesh, view::Visibility},
    sprite::{ColorMaterial, MaterialMesh2dBundle},
    transform::components::Transform,
    utils::default,
};
use bevy_rapier2d::{
    dynamics::RigidBody,
    geometry::{CollisionGroups, Group},
};

use crate::{
    edge_wrap::Duplicable,
    ship::{Ship, Thruster},
    utils::mesh_to_collider,
};

#[derive(Component)]
pub struct Player;

pub const PLAYER_GROUP: Group = Group::GROUP_1;
pub const PLAYER_FILTER: Group = Group::ALL;

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

const PLAYER_SHIP_TIP_Y: f32 = 20.;
const PLAYER_SHIP_SIDE_Y: f32 = -14.;
const PLAYER_SHIP_SIDE_X: f32 = 14.;

fn spawn_player_ship(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    transform: Transform,
) {
    let player_shape = Triangle2d::new(
        Vec2::new(0., PLAYER_SHIP_TIP_Y),
        Vec2::new(-PLAYER_SHIP_SIDE_X, PLAYER_SHIP_SIDE_Y),
        Vec2::new(PLAYER_SHIP_SIDE_X, PLAYER_SHIP_SIDE_Y),
    );

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
            CollisionGroups::new(PLAYER_GROUP, PLAYER_FILTER),
        ))
        .with_children(|parent| {
            for x in [-9., 0., 9.] {
                parent.spawn((
                    Name::new("Thruster"),
                    Thruster,
                    MaterialMesh2dBundle {
                        transform: Transform::from_translation(Vec3::new(
                            x,
                            PLAYER_SHIP_SIDE_Y - 2.,
                            -1.,
                        )),
                        mesh: meshes.add(Mesh::from(RegularPolygon::new(6., 3))).into(),
                        material: materials.add(ColorMaterial::from(Color::RED)),
                        visibility: Visibility::Hidden,
                        ..default()
                    },
                ));
            }
        });
}
