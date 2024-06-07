use bevy::{
    app::{App, Plugin, Startup, Update},
    asset::{Assets, Handle},
    core::Name,
    ecs::{
        component::Component,
        entity::Entity,
        event::{Event, EventReader, EventWriter},
        query::With,
        schedule::{IntoSystemConfigs, SystemSet},
        system::{Commands, EntityCommand, EntityCommands, Query, Res, ResMut, Resource},
        world::{Mut, World},
    },
    hierarchy::{BuildWorldChildren, Children, DespawnRecursiveExt},
    log::{info, warn},
    math::{
        primitives::{RegularPolygon, Triangle2d},
        FloatExt, Vec2, Vec3, Vec3Swizzles,
    },
    render::{color::Color, mesh::Mesh, view::Visibility},
    sprite::{ColorMaterial, MaterialMesh2dBundle, Mesh2dHandle},
    time::Time,
    transform::components::Transform,
    utils::default,
};
use bevy_rapier2d::{
    dynamics::{ExternalImpulse, RigidBody, Velocity},
    geometry::{CollisionGroups, Group},
    plugin::RapierContext,
    prelude::CollisionEvent,
};

use crate::{
    asteroid::{Asteroid, SplitAsteroidEvent},
    edge_wrap::Duplicable,
    explosion::{self, spawn_explosion, ExplosionEvent},
    shatter::spawn_shattered_mesh,
    utils::{contact_position_and_normal, mesh_to_collider},
};

pub struct ShipPlugin;

impl Plugin for ShipPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ShipDestroyedEvent>()
            .add_systems(Startup, load_ship_material)
            .add_systems(
                Update,
                (ship_movement, ship_asteroid_collision, explode_ship)
                    .chain()
                    .in_set(ShipSet),
            );
    }
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct ShipSet;

#[derive(Resource)]
struct ShipMaterial(Handle<ColorMaterial>);

fn load_ship_material(mut commands: Commands, mut materials: ResMut<Assets<ColorMaterial>>) {
    commands.insert_resource(ShipMaterial(
        materials.add(ColorMaterial::from(Color::WHITE)),
    ));
}

#[derive(Component)]
pub struct Ship;

#[derive(Component)]
pub struct Thruster;

pub const SHIP_GROUP: Group = Group::GROUP_1;
pub const SHIP_FILTER: Group = Group::ALL;
const SHIP_TIP_Y: f32 = 20.;
const SHIP_SIDE_Y: f32 = -14.;
const SHIP_SIDE_X: f32 = 14.;

struct SpawnShip {
    transform: Transform,
}

impl EntityCommand for SpawnShip {
    fn apply(self, entity: Entity, world: &mut World) {
        let (ship_mesh_handle, collider) =
            world.resource_scope(|_world, mut meshes: Mut<Assets<Mesh>>| {
                let mesh = Mesh::from(Triangle2d::new(
                    Vec2::new(0., SHIP_TIP_Y),
                    Vec2::new(-SHIP_SIDE_X, SHIP_SIDE_Y),
                    Vec2::new(SHIP_SIDE_X, SHIP_SIDE_Y),
                ));

                let collider = mesh_to_collider(&mesh).expect("Failed to create collider");
                (meshes.add(mesh), collider)
            });

        let ship_material_handle =
            world.resource_scope(|_world, mut materials: Mut<Assets<ColorMaterial>>| {
                materials.add(ColorMaterial::from(Color::WHITE))
            });

        let thruster_mesh_handle: Mesh2dHandle = world
            .resource_scope(|_world, mut meshes: Mut<Assets<Mesh>>| {
                meshes.add(Mesh::from(RegularPolygon::new(6., 3)))
            })
            .into();

        let thruster_material_handle =
            world.resource_scope(|_world, mut materials: Mut<Assets<ColorMaterial>>| {
                materials.add(ColorMaterial::from(Color::RED))
            });

        world
            .entity_mut(entity)
            .insert((
                Ship,
                MaterialMesh2dBundle {
                    mesh: ship_mesh_handle.into(),
                    material: ship_material_handle,
                    transform: self.transform,
                    ..default()
                },
                RigidBody::Dynamic,
                collider,
                Duplicable,
                CollisionGroups::new(SHIP_GROUP, SHIP_FILTER),
            ))
            .with_children(|parent| {
                for x in [-9., 0., 9.] {
                    parent.spawn((
                        Name::new("Thruster"),
                        Thruster,
                        MaterialMesh2dBundle {
                            transform: Transform::from_translation(Vec3::new(
                                x,
                                SHIP_SIDE_Y - 2.,
                                -1.,
                            )),
                            mesh: thruster_mesh_handle.clone(),
                            material: thruster_material_handle.clone(),
                            visibility: Visibility::Hidden,
                            ..default()
                        },
                    ));
                }
            });
    }
}

pub trait SpawnShipExt {
    fn spawn_ship(&mut self, transform: Transform) -> EntityCommands;
}

impl<'w, 's> SpawnShipExt for Commands<'w, 's> {
    fn spawn_ship(&mut self, transform: Transform) -> EntityCommands {
        let mut e = self.spawn_empty();
        e.add(SpawnShip { transform });
        e
    }
}

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
    rapier_context: Res<RapierContext>,
    mut collision_events: EventReader<CollisionEvent>,
    ship_query: Query<(&Transform, Option<&Velocity>, &mut Mesh2dHandle), With<Ship>>,
    asteroid_query: Query<Entity, With<Asteroid>>,
    mut ship_destroyed_events: EventWriter<ShipDestroyedEvent>,
    mut split_asteroid_events: EventWriter<SplitAsteroidEvent>,
) {
    for event in collision_events.read() {
        if let CollisionEvent::Started(entity_a, entity_b, _) = event {
            let (ship_entity, asteroid_entity) =
                if ship_query.contains(*entity_a) && asteroid_query.contains(*entity_b) {
                    (*entity_a, *entity_b)
                } else if ship_query.contains(*entity_b) && asteroid_query.contains(*entity_a) {
                    (*entity_b, *entity_a)
                } else {
                    continue;
                };
            info!("Ship collided with asteroid");

            ship_destroyed_events.send(ShipDestroyedEvent { ship_entity });

            let Some((collision_position, collision_direction)) =
                contact_position_and_normal(&rapier_context, ship_entity, asteroid_entity)
            else {
                warn!("No collision position found");
                continue;
            };

            split_asteroid_events.send(SplitAsteroidEvent {
                asteroid_entity,
                collision_direction,
                collision_position,
            });
        }
    }
}

fn explode_ship(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    ship_material: Res<ShipMaterial>,
    mut ship_destroyed_events: EventReader<ShipDestroyedEvent>,
    ship_query: Query<(&Transform, Option<&Velocity>, &mut Mesh2dHandle), With<Ship>>,
    mut explosion_events: EventWriter<ExplosionEvent>,
) {
    for ShipDestroyedEvent { ship_entity } in ship_destroyed_events.read() {
        let (ship_transform, ship_velocity, ship_mesh_handle) =
            ship_query.get(*ship_entity).unwrap();

        let mesh = meshes
            .get(&ship_mesh_handle.0)
            .expect("Ship mesh not found")
            .clone();

        spawn_shattered_mesh(
            &mesh,
            ship_material.0.clone(),
            ship_transform,
            ship_velocity.copied().unwrap_or_else(Velocity::zero),
            &mut commands,
            &mut meshes,
        );
        explosion_events.send(ExplosionEvent {
            position: ship_transform.translation.xy(),
            radius: 6.,
        });

        commands.entity(*ship_entity).despawn_recursive();
    }
}
