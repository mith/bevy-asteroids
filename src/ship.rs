use bevy::{
    app::{App, Plugin, Update},
    asset::{Assets, Handle},
    ecs::{
        component::Component,
        entity::Entity,
        event::{Event, EventReader, EventWriter},
        query::With,
        schedule::{apply_deferred, IntoSystemConfigs, SystemSet},
        system::{Commands, Query, Res, ResMut},
    },
    hierarchy::{Children, DespawnRecursiveExt},
    log::info,
    math::{FloatExt, Vec3, Vec3Swizzles},
    render::{
        mesh::Mesh,
        view::{visibility, Visibility},
    },
    sprite::{ColorMaterial, Mesh2dHandle},
    time::Time,
    transform::components::Transform,
    utils::default,
};
use bevy_rapier2d::{
    dynamics::{ExternalImpulse, Velocity},
    prelude::CollisionEvent,
};

use crate::asteroid::spawn_shattered_mesh;

pub struct ShipPlugin;

impl Plugin for ShipPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ShipDestroyedEvent>().add_systems(
            Update,
            (ship_movement, apply_deferred, ship_asteroid_collision)
                .chain()
                .in_set(ShipSet),
        );
    }
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct ShipSet;

#[derive(Component)]
pub struct Ship;

#[derive(Component)]
pub struct Thruster;

#[derive(Component)]
pub struct Throttling;

pub fn ship_movement(
    mut commands: Commands,
    ship_query: Query<(Entity, &Transform, Option<&Throttling>, &Children), With<Ship>>,
    mut thruster_query: Query<&mut Handle<ColorMaterial>, With<Thruster>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    time: Res<Time>,
) {
    let ship_power = 500.;

    for (ship_entity, global_transform, throttling, children) in &ship_query {
        if throttling.is_some() {
            let force = global_transform
                .rotation
                .mul_vec3(Vec3::new(0., 1., 0.))
                .xy();

            commands.entity(ship_entity).insert(ExternalImpulse {
                impulse: force * ship_power,
                ..default()
            });
            set_thrusters_visibility(
                &mut commands,
                children,
                &mut thruster_query,
                &mut materials,
                &time,
                Visibility::Visible,
            );
        } else {
            commands.entity(ship_entity).remove::<ExternalImpulse>();
            set_thrusters_visibility(
                &mut commands,
                children,
                &mut thruster_query,
                &mut materials,
                &time,
                Visibility::Hidden,
            )
        }
    }
}

fn set_thrusters_visibility(
    commands: &mut Commands,
    children: &Children,
    thruster_query: &mut Query<&mut Handle<ColorMaterial>, With<Thruster>>,
    materials: &mut Assets<ColorMaterial>,
    time: &Time,
    visibility: Visibility,
) {
    let thruster_fade_speed = match visibility {
        Visibility::Visible => 1.,
        Visibility::Hidden => 2.,
        _ => unimplemented!(),
    };

    for child_entity in children.iter() {
        let Ok(thruster_material_handle) = thruster_query.get_mut(*child_entity) else {
            continue;
        };

        let thruster_material = materials.get_mut(thruster_material_handle.clone()).unwrap();

        let thruster_transparency = thruster_material.color.a();
        thruster_material.color = thruster_material.color.with_a(thruster_transparency.lerp(
            match visibility {
                Visibility::Visible => 1.,
                Visibility::Hidden => 0.,
                _ => unimplemented!(),
            },
            time.delta_seconds() * thruster_fade_speed,
        ));

        commands.entity(*child_entity).insert(visibility);
    }
}

#[derive(Event)]
pub struct ShipDestroyedEvent {
    pub ship_entity: Entity,
}

fn ship_asteroid_collision(
    mut commands: Commands,
    mut collision_events: EventReader<CollisionEvent>,
    ship_query: Query<(Entity, &Transform, Option<&Velocity>, &mut Mesh2dHandle), With<Ship>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut ship_destroyed_events: EventWriter<ShipDestroyedEvent>,
) {
    for event in collision_events.read() {
        if let CollisionEvent::Started(entity_a, entity_b, _) = event {
            let Some((player_entity, player_transform, player_velocity, player_mesh_handle)) =
                ship_query.get_single().ok()
            else {
                return;
            };
            if player_entity == *entity_a || player_entity == *entity_b {
                info!("Ship collided with asteroid");

                let mesh = meshes
                    .get(&player_mesh_handle.0)
                    .expect("Ship mesh not found")
                    .clone();

                spawn_shattered_mesh(
                    &mesh,
                    player_transform,
                    player_velocity.copied().unwrap_or_else(Velocity::zero),
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                );

                commands.entity(player_entity).despawn_recursive();

                ship_destroyed_events.send(ShipDestroyedEvent {
                    ship_entity: player_entity,
                });
            }
        }
    }
}
