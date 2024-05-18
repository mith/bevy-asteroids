use bevy::{
    asset::Assets,
    ecs::{
        component::Component,
        entity::Entity,
        event::{Event, EventReader},
        system::{Commands, Query, Res, ResMut},
    },
    math::{primitives::RegularPolygon, Vec2, Vec3, Vec3Swizzles},
    render::{color::Color, mesh::Mesh},
    sprite::{ColorMaterial, MaterialMesh2dBundle},
    time::{Time, Timer},
    transform::components::Transform,
    utils::default,
};
use bevy_rapier2d::dynamics::{RigidBody, Velocity};

use crate::{edge_wrap::Duplicable, utils::mesh_to_collider};

#[derive(Event, Debug, Clone, Copy)]
pub struct FireEvent {
    pub turret_entity: Entity,
}

#[derive(Component)]
pub struct ReloadTimer(Timer);

const RELOAD_DURATION: f32 = 0.3;

impl Default for ReloadTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(
            RELOAD_DURATION,
            bevy::time::TimerMode::Once,
        ))
    }
}

pub fn reload(
    mut commands: Commands,
    mut reload_timer_query: Query<(Entity, &mut ReloadTimer)>,
    time: Res<Time>,
) {
    for (entity, mut reload_timer) in reload_timer_query.iter_mut() {
        if reload_timer.0.tick(time.delta()).just_finished() {
            commands.entity(entity).remove::<ReloadTimer>();
        }
    }
}

#[derive(Component)]
pub struct Projectile;

pub fn fire_projectile(
    mut commands: Commands,
    mut fire_event_reader: EventReader<FireEvent>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    transform_query: Query<&Transform>,
    reload_timer_query: Query<&ReloadTimer>,
) {
    for FireEvent { turret_entity } in fire_event_reader.read() {
        if reload_timer_query.get(*turret_entity).is_ok() {
            continue;
        }
        commands
            .entity(*turret_entity)
            .insert(ReloadTimer::default());
        let turret_transform = transform_query.get(*turret_entity).unwrap();

        let position = turret_transform.translation.xy()
            + turret_transform
                .rotation
                .mul_vec3(Vec3::new(0., 10., 0.))
                .xy();

        let velocity = turret_transform
            .rotation
            .mul_vec3(Vec3::new(0., 1., 0.))
            .xy()
            * 500.;
        spawn_projectile(
            &mut commands,
            &mut meshes,
            &mut materials,
            position,
            velocity,
        );
    }
}

fn spawn_projectile(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    position: Vec2,
    velocity: Vec2,
) {
    let projectile_shape = RegularPolygon::new(5., 3);

    let projectile_mesh = Mesh::from(projectile_shape);
    let collider = mesh_to_collider(&projectile_mesh);
    commands.spawn((
        Projectile,
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
    ));
}
