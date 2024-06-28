use bevy::{
    ecs::{
        component::Component,
        entity::Entity,
        query::With,
        system::{Commands, Query, Res},
    },
    gizmos::gizmos::Gizmos,
    math::{Vec2, Vec3Swizzles},
    prelude::default,
    render::color::Color,
    time::Time,
    transform::components::GlobalTransform,
};
use bevy_rapier2d::{
    dynamics::Velocity,
    geometry::{Collider, CollisionGroups, Group, ShapeCastOptions},
    pipeline::QueryFilter,
    plugin::RapierContext,
};
use rand::Rng;
use serde::Deserialize;

use crate::{asteroid::ASTEROID_GROUP, projectile::PROJECTILE_GROUP};

use super::{KillTarget, Ufo, UfoSettings, UFO_GROUP};

#[derive(Component, Debug, Deserialize, Default, Clone)]
pub struct AvoidanceWeights {
    forward_threat_avoidance_weight: f32,
    surrounding_threat_avoidance_weight: f32,
    incoming_threat_avoidance_weight: f32,
}

pub fn move_ufo(
    mut commands: Commands,
    mut ufo_query: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&Velocity>,
            &Collider,
            &AvoidanceWeights,
            Option<&KillTarget>,
        ),
        With<Ufo>,
    >,
    transform_query: Query<&GlobalTransform>,
    collider_query: Query<(&GlobalTransform, Option<&Velocity>, &Collider)>,
    time: Res<Time>,
    rapier_context: Res<RapierContext>,
    ufo_settings: Res<UfoSettings>,
    mut gizmos: Gizmos,
) {
    let mut rng = rand::thread_rng();

    for (
        ufo_entity,
        ufo_transform,
        opt_ufo_velocity,
        ufo_collider,
        avoidance_weights,
        opt_target,
    ) in ufo_query.iter_mut()
    {
        let target_impulse_strength = if let Some(KillTarget(target_entity)) = opt_target {
            calculate_target_impulse(*target_entity, &transform_query, ufo_transform, &mut rng)
        } else {
            Vec2::ZERO
        };

        // Check for nearby obstacles to avoid
        let avoidance_impulse_strength = calculate_avoidance_impulse(
            &rapier_context,
            ufo_entity,
            ufo_transform,
            opt_ufo_velocity.unwrap_or(&Velocity::zero()),
            ufo_collider,
            &collider_query,
            avoidance_weights,
            ufo_settings.debug_enabled.then_some(&mut gizmos),
        );

        let dampen_impulse = if avoidance_impulse_strength.length() < 10. {
            -opt_ufo_velocity.map_or(Vec2::ZERO, |velocity| velocity.linvel) * 0.001
        } else {
            Vec2::ZERO
        };

        let impulse_strength =
            target_impulse_strength + avoidance_impulse_strength + dampen_impulse;

        let old_velocity = opt_ufo_velocity.map_or(Vec2::ZERO, |velocity| velocity.linvel);
        let new_velocity = impulse_strength * 50_000. * time.delta_seconds();

        let max_acceleration = Vec2::splat(ufo_settings.max_acceleration);
        let new_velocity = new_velocity.clamp(
            old_velocity - max_acceleration * time.delta_seconds(),
            old_velocity + max_acceleration * time.delta_seconds(),
        );

        let velocity = (old_velocity * 2. + new_velocity) / 3.;
        let max_velocity = Vec2::splat(ufo_settings.max_velocity);

        // Apply the final impulse
        commands.entity(ufo_entity).insert(Velocity {
            linvel: velocity.clamp(-max_velocity, max_velocity),
            ..default()
        });
    }
}

fn calculate_avoidance_impulse(
    rapier_context: &Res<RapierContext>,
    ufo_entity: Entity,
    ufo_transform: &GlobalTransform,
    ufo_velocity: &Velocity,
    ufo_collider: &Collider,
    collider_query: &Query<(&GlobalTransform, Option<&Velocity>, &Collider)>,
    avoidance_weights: &AvoidanceWeights,
    mut gizmos: Option<&mut Gizmos>,
) -> Vec2 {
    let mut avoid_direction = Vec2::ZERO;

    avoid_direction += avoid_forward_threat(
        rapier_context,
        ufo_transform,
        ufo_velocity,
        ufo_collider,
        ufo_entity,
        collider_query,
        &mut gizmos,
    ) * avoidance_weights.forward_threat_avoidance_weight;

    let (avoid_surrounding_direction, avoid_incoming_direction) =
        avoid_surrounding_threats(collider_query, ufo_transform, rapier_context, &mut gizmos);

    avoid_direction +=
        avoid_surrounding_direction * avoidance_weights.surrounding_threat_avoidance_weight;
    avoid_direction +=
        avoid_incoming_direction * avoidance_weights.incoming_threat_avoidance_weight;

    avoid_direction
}

fn avoid_surrounding_threats(
    collider_query: &Query<(&GlobalTransform, Option<&Velocity>, &Collider), ()>,
    ufo_transform: &GlobalTransform,
    rapier_context: &Res<RapierContext>,
    gizmos: &mut Option<&mut Gizmos<bevy::prelude::DefaultGizmoConfigGroup>>,
) -> (Vec2, Vec2) {
    let mut avoid_surrounding_direction = Vec2::ZERO;
    let mut avoid_incoming_direction = Vec2::ZERO;
    let collider = Collider::ball(300.);
    let mut intersections = vec![];
    rapier_context.intersections_with_shape(
        ufo_transform.translation().xy(),
        0.,
        &collider,
        QueryFilter::new().groups(CollisionGroups::new(
            UFO_GROUP,
            ASTEROID_GROUP | PROJECTILE_GROUP,
        )),
        |e| {
            intersections.push(e);
            true
        },
    );

    for intersection_entity in intersections {
        let (asteroid_transform, opt_asteroid_velocity, asteroid_collider) = collider_query
            .get(intersection_entity)
            .expect("Asteroid collider not found");

        let asteroid_ufo_distance =
            asteroid_transform.translation().xy() - ufo_transform.translation().xy();

        if let Some(asteroid_velocity) = opt_asteroid_velocity {
            avoid_incoming_direction += avoid_moving_threat(
                rapier_context,
                asteroid_transform,
                asteroid_velocity,
                asteroid_collider,
                intersection_entity,
                asteroid_ufo_distance,
                ufo_transform,
                gizmos,
            );
        }

        let weight = 1. / asteroid_ufo_distance.length().powi(3);
        let asteroid_ufo_direction = asteroid_ufo_distance.normalize();
        let avoidance_impulse = -asteroid_ufo_direction * weight;
        let start = ufo_transform.translation().xy();
        if let Some(gizmos) = gizmos.as_mut() {
            gizmos.line_2d(start, start + avoidance_impulse, Color::GREEN);
            gizmos.circle_2d(asteroid_transform.translation().xy(), 20., Color::GREEN);
        }
        avoid_surrounding_direction += avoidance_impulse;
    }
    (avoid_surrounding_direction, avoid_incoming_direction)
}

fn avoid_moving_threat(
    rapier_context: &Res<RapierContext>,
    asteroid_transform: &GlobalTransform,
    asteroid_velocity: &Velocity,
    asteroid_collider: &Collider,
    intersection_entity: Entity,
    asteroid_ufo_distance: Vec2,
    ufo_transform: &GlobalTransform,
    gizmos: &mut Option<&mut Gizmos<bevy::prelude::DefaultGizmoConfigGroup>>,
) -> Vec2 {
    let mut avoid_direction = Vec2::ZERO;
    if rapier_context
        .cast_shape(
            asteroid_transform.translation().xy(),
            0.,
            asteroid_velocity.linvel,
            asteroid_collider,
            ShapeCastOptions {
                max_time_of_impact: 4.,
                ..default()
            },
            QueryFilter::new()
                .exclude_collider(intersection_entity)
                .groups(CollisionGroups::new(Group::all(), UFO_GROUP)),
        )
        .is_some()
    {
        let vel_normal = asteroid_velocity.linvel.normalize_or_zero();
        let normal = Vec2::new(-vel_normal.y, vel_normal.x); // Normal of the velocity
        let asteroid_ufo_direction = asteroid_ufo_distance.normalize();
        let dot_product = asteroid_ufo_direction.dot(normal);

        let weight =
            asteroid_velocity.linvel.length_squared() + 1. / asteroid_ufo_distance.length_squared();

        // Adjust direction based on which side of the normal the UFO is on
        let avoidance_impulse = if dot_product > 0. { -normal } else { normal } * weight;

        let start = ufo_transform.translation().xy();
        if let Some(gizmos) = gizmos.as_mut() {
            gizmos.line_2d(start, start + avoidance_impulse, Color::RED);
            gizmos.circle_2d(asteroid_transform.translation().xy(), 30., Color::RED);
        }
        avoid_direction += avoidance_impulse;
    }
    avoid_direction
}

fn avoid_forward_threat(
    rapier_context: &Res<RapierContext>,
    ufo_transform: &GlobalTransform,
    ufo_velocity: &Velocity,
    ufo_collider: &Collider,
    ufo_entity: Entity,
    collider_query: &Query<(&GlobalTransform, Option<&Velocity>, &Collider)>,
    gizmos: &mut Option<&mut Gizmos<bevy::prelude::DefaultGizmoConfigGroup>>,
) -> Vec2 {
    let mut avoid_direction = Vec2::ZERO;
    if let Some((collision_entity, _)) = rapier_context.cast_shape(
        ufo_transform.translation().xy(),
        0.,
        ufo_velocity.linvel,
        ufo_collider,
        ShapeCastOptions {
            max_time_of_impact: 0.5,
            ..default()
        },
        QueryFilter::new()
            .exclude_collider(ufo_entity)
            .groups(CollisionGroups::new(
                UFO_GROUP,
                ASTEROID_GROUP | PROJECTILE_GROUP,
            )),
    ) {
        let (asteroid_transform, _, _) = collider_query
            .get(collision_entity)
            .expect("Asteroid collider not found");
        let asteroid_ufo_distance =
            asteroid_transform.translation().xy() - ufo_transform.translation().xy();
        let vel_normal = ufo_velocity.linvel.normalize_or_zero();
        let normal = Vec2::new(-vel_normal.y, vel_normal.x); // Normal of the velocity
        let asteroid_ufo_direction = asteroid_ufo_distance.normalize();
        let dot_product = asteroid_ufo_direction.dot(normal);
        let weight =
            ufo_velocity.linvel.length_squared() + 1. / asteroid_ufo_distance.length_squared();

        // Adjust direction based on which side of the normal the UFO is on
        let avoidance_impulse = if dot_product > 0. { -normal } else { normal } * weight * 500.;
        let start = ufo_transform.translation().xy();
        if let Some(gizmos) = gizmos.as_mut() {
            gizmos.line_2d(start, start + avoidance_impulse, Color::ORANGE);
            gizmos.circle_2d(asteroid_transform.translation().xy(), 40., Color::ORANGE);
        }
        avoid_direction += avoidance_impulse;
    }
    avoid_direction
}

fn calculate_target_impulse(
    target: Entity,
    transform_query: &Query<&GlobalTransform>,
    ufo_transform: &GlobalTransform,
    rng: &mut rand::prelude::ThreadRng,
) -> Vec2 {
    if let Ok(target_transform) = transform_query.get(target) {
        let target_xy_distance =
            (target_transform.translation() - ufo_transform.translation()).xy();
        let target_distance = target_xy_distance.length();

        // Calculate impulse strength based on distance to the target
        let min_distance = 400.0;
        let max_distance = 600.;

        if target_distance < 1.0 {
            Vec2::new(rng.gen_range(-1.0..1.0), rng.gen_range(-1.0..1.0))
        } else if target_distance < min_distance {
            -target_xy_distance.normalize()
        } else if target_distance > max_distance {
            target_xy_distance.normalize()
        } else {
            let ufo_translation = ufo_transform.translation().xy();
            -ufo_translation.normalize_or_zero() * 00.1
        }
    } else {
        Vec2::ZERO
    }
}
