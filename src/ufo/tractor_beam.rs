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
    time::{Time, Timer, TimerMode},
    transform::components::GlobalTransform,
};
use bevy_rapier2d::dynamics::{ExternalImpulse, ReadMassProperties};
use rand::Rng;

use crate::{asteroid::Asteroid, player::Player};

use super::{InsideBounds, Ufo};

const TRACTOR_BEAM_RELOAD_TIME: f32 = 4.;
const TRACTOR_BEAM_ARMED_TIME: f32 = 2.;
const TRACTOR_BEAM_FORCE: f32 = 250000.;

enum TractorBeamState {
    Armed(Timer),
    Reloading(Timer),
}

#[derive(Component)]
pub struct TractorBeam {
    state: TractorBeamState,
}

impl Default for TractorBeam {
    fn default() -> Self {
        Self {
            state: TractorBeamState::Armed(Timer::from_seconds(
                TRACTOR_BEAM_ARMED_TIME,
                TimerMode::Once,
            )),
        }
    }
}

pub fn throw_asteroid(
    mut commands: Commands,
    mut ufo_query: Query<(&mut TractorBeam, &GlobalTransform), (With<Ufo>, With<InsideBounds>)>,
    asteroid_query: Query<(Entity, &GlobalTransform, &ReadMassProperties), With<Asteroid>>,
    player_query: Query<&GlobalTransform, With<Player>>,
    mut gizmos: Gizmos,
    time: Res<Time>,
) {
    let Ok(player_transform) = player_query.get_single() else {
        return;
    };

    for (mut tractor_beam, ufo_transform) in ufo_query.iter_mut() {
        update_tractor_beam_state(&mut tractor_beam, &time);
        if matches!(tractor_beam.state, TractorBeamState::Reloading(_)) {
            continue;
        }
        let closest_asteroid =
            find_suitable_asteroid(&asteroid_query, ufo_transform, player_transform);

        if let Some((asteroid_entity, asteroid_position)) = closest_asteroid {
            let direction_to_player = player_transform.translation().xy() - asteroid_position;

            if direction_to_player.length() < 100. {
                return;
            }

            gizmos.line_2d(
                ufo_transform.translation().xy(),
                asteroid_position,
                Color::BLUE,
            );

            commands.entity(asteroid_entity).insert(ExternalImpulse {
                impulse: direction_to_player.normalize()
                    * TRACTOR_BEAM_FORCE
                    * time.delta_seconds(),
                ..default()
            });
        }
    }
}

fn find_suitable_asteroid(
    asteroid_query: &Query<(Entity, &GlobalTransform, &ReadMassProperties), With<Asteroid>>,
    ufo_transform: &GlobalTransform,
    player_transform: &GlobalTransform,
) -> Option<(Entity, Vec2)> {
    asteroid_query
        .iter()
        .filter(|(_, asteroid_transform, _)| {
            let asteroid_ufo_distance = asteroid_transform
                .translation()
                .xy()
                .distance(ufo_transform.translation().xy());

            let asteroid_player_distance = asteroid_transform
                .translation()
                .xy()
                .distance(player_transform.translation().xy());
            asteroid_ufo_distance < 500. && asteroid_player_distance > 100.
        })
        .min_by_key(|(_, asteroid_transform, mass_properties)| {
            let asteroid_ufo_distance = asteroid_transform
                .translation()
                .xy()
                .distance(ufo_transform.translation().xy());

            let asteroid_player_distance = asteroid_transform
                .translation()
                .xy()
                .distance(player_transform.translation().xy());
            asteroid_ufo_distance as i32 * 2
                + asteroid_player_distance as i32
                + (mass_properties.get().mass * 0.5) as i32
        })
        .map(|(entity, asteroid_transform, _)| (entity, asteroid_transform.translation().xy()))
}

fn update_tractor_beam_state(tractor_beam: &mut TractorBeam, time: &Res<Time>) {
    match tractor_beam.state {
        TractorBeamState::Armed(ref mut timer) => {
            if timer.tick(time.delta()).just_finished() {
                tractor_beam.state = TractorBeamState::Reloading(Timer::from_seconds(
                    TRACTOR_BEAM_RELOAD_TIME + rand::thread_rng().gen_range(0.0..1.0),
                    TimerMode::Once,
                ));
            }
        }
        TractorBeamState::Reloading(ref mut timer) => {
            if timer.tick(time.delta()).just_finished() {
                tractor_beam.state = TractorBeamState::Armed(Timer::from_seconds(
                    TRACTOR_BEAM_ARMED_TIME + rand::thread_rng().gen_range(0.0..1.0),
                    TimerMode::Once,
                ));
            }
        }
    }
}
