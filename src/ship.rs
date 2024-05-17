use bevy::{
    asset::{Assets, Handle},
    ecs::{
        component::Component,
        entity::Entity,
        query::With,
        system::{Commands, Query, Res, ResMut},
    },
    hierarchy::Children,
    math::{FloatExt, Vec3, Vec3Swizzles},
    render::view::Visibility,
    sprite::ColorMaterial,
    time::Time,
    transform::components::Transform,
    utils::default,
};
use bevy_rapier2d::dynamics::ExternalImpulse;

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
    let thruster_fade_speed = 1.;

    for (ship_entity, global_transform, throttling, children) in &ship_query {
        let thruster_entity = children
            .iter()
            .find(|child_entity| thruster_query.contains(**child_entity))
            .copied()
            .unwrap();

        let thruster_material_handle = thruster_query.get_mut(thruster_entity).unwrap();

        let thruster_material = materials.get_mut(thruster_material_handle.clone()).unwrap();

        if throttling.is_some() {
            let force = global_transform
                .rotation
                .mul_vec3(Vec3::new(0., 1., 0.))
                .xy();

            commands.entity(ship_entity).insert(ExternalImpulse {
                impulse: force * ship_power,
                ..default()
            });
            commands.entity(thruster_entity).insert(Visibility::Visible);

            let thruster_transparency = thruster_material.color.a();
            thruster_material.color = thruster_material
                .color
                .with_a(thruster_transparency.lerp(1., time.delta_seconds() * thruster_fade_speed));
        } else {
            commands.entity(ship_entity).remove::<ExternalImpulse>();
            commands.entity(thruster_entity).insert(Visibility::Hidden);

            let thruster_transparency = thruster_material.color.a();
            thruster_material.color = thruster_material.color.with_a(
                thruster_transparency.lerp(0., time.delta_seconds() * thruster_fade_speed * 2.),
            );
        }
    }
}
