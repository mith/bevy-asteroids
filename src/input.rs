use bevy::{
    app::{App, Plugin, Update},
    ecs::{
        entity::Entity,
        event::EventWriter,
        query::With,
        schedule::{
            common_conditions::{in_state, not, resource_exists_and_equals},
            IntoSystemConfigs, SystemSet,
        },
        system::{Commands, Query, Res, Resource},
    },
    input::{mouse::MouseButton, touch::Touches, ButtonInput},
    math::{Quat, Vec2},
    prelude::{OnExit, ResMut},
    render::camera::Camera,
    time::{Time, Timer, TimerMode},
    transform::components::{GlobalTransform, Transform},
    window::{PrimaryWindow, Window},
};

use crate::{
    ship::{Ship, Throttling},
    turret::FireEvent,
    utils::cleanup_resource,
    GameState, Player,
};

pub struct PlayerInputPlugin;

impl Plugin for PlayerInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (
                    player_ship_mouse_input.run_if(resource_exists_and_equals(InputMode::Mouse)),
                    (touch_shoot_timer_update, player_ship_touch_input)
                        .chain()
                        .run_if(resource_exists_and_equals(InputMode::Touch)),
                )
                    .run_if(in_state(GameState::Playing)),
                stop_player_throttling.run_if(not(in_state(GameState::Playing))),
            )
                .in_set(PlayerInputSet),
        )
        .add_systems(
            OnExit(GameState::Playing),
            cleanup_resource::<TouchShootTimer>,
        );
    }
}

#[derive(SystemSet, PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub struct PlayerInputSet;

#[derive(Resource, PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub enum InputMode {
    Mouse,
    Touch,
}

pub fn player_ship_mouse_input(
    mut commands: Commands,
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut player_query: Query<(Entity, &GlobalTransform, &mut Transform), (With<Player>, With<Ship>)>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    mut fire_projectile_event_writer: EventWriter<FireEvent>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
) {
    let (camera, camera_global_transform) = camera_query.single();
    let Some(cursor_pos) = primary_window
        .single()
        .cursor_position()
        .and_then(|cp| camera.viewport_to_world_2d(camera_global_transform, cp))
    else {
        return;
    };

    for (player_entity, player_global_transform, mut player_transform) in player_query.iter_mut() {
        let throttle = mouse_input.pressed(MouseButton::Left);

        if throttle {
            commands.entity(player_entity).insert(Throttling);
        } else {
            commands.entity(player_entity).remove::<Throttling>();
        }

        let direction = cursor_pos - player_global_transform.translation().truncate();
        let angle = direction.y.atan2(direction.x);
        let target_rotation = Quat::from_rotation_z(angle - std::f32::consts::FRAC_PI_2);

        player_transform.rotation = target_rotation;

        let fire_projectile = mouse_input.pressed(MouseButton::Right);

        if fire_projectile {
            fire_projectile_event_writer.send(FireEvent {
                turret_entity: player_entity,
            });
        }
    }
}

#[derive(Resource)]
struct TouchShootTimer {
    timer: Timer,
    position: Vec2,
}

impl TouchShootTimer {
    fn new(position: Vec2) -> Self {
        Self {
            timer: Timer::from_seconds(0.3, TimerMode::Once),
            position,
        }
    }
}

fn touch_shoot_timer_update(
    mut commands: Commands,
    mut timer: Option<ResMut<TouchShootTimer>>,
    time: Res<Time>,
) {
    if let Some(timer) = timer.as_mut() {
        if timer.timer.tick(time.delta()).just_finished() {
            commands.remove_resource::<TouchShootTimer>();
        }
    }
}

fn player_ship_touch_input(
    mut commands: Commands,
    touches: Res<Touches>,
    mut player_query: Query<(Entity, &GlobalTransform, &mut Transform), (With<Player>, With<Ship>)>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    mut fire_projectile_event_writer: EventWriter<FireEvent>,
    touch_shoot_timer: Option<Res<TouchShootTimer>>,
) {
    let (camera, camera_global_transform) = camera_query.single();

    let Ok((player_entity, player_global_transform, mut player_transform)) =
        player_query.get_single_mut()
    else {
        return;
    };

    if let Some(touch) = touches.first_pressed_position() {
        if let Some(timer) = touch_shoot_timer {
            if timer.position.distance_squared(touch) < 1000.0 {
                fire_projectile_event_writer.send(FireEvent {
                    turret_entity: player_entity,
                });
                commands.remove_resource::<TouchShootTimer>();
                return;
            }
        }
        let touch_world_pos = camera
            .viewport_to_world_2d(camera_global_transform, touch)
            .expect("Touch position not in world coordinates");
        {
            // Point ship towards touch location
            let direction = touch_world_pos - player_global_transform.translation().truncate();
            let angle = direction.y.atan2(direction.x);
            let target_rotation = Quat::from_rotation_z(angle - std::f32::consts::FRAC_PI_2);
            player_transform.rotation = target_rotation;

            for (player_entity, _, _) in player_query.iter() {
                commands.entity(player_entity).insert(Throttling);
            }
        }
    } else if let Some(touch) = touches.iter_just_released().next() {
        for (player_entity, _, _) in player_query.iter() {
            commands.entity(player_entity).remove::<Throttling>();
        }
        commands.insert_resource(TouchShootTimer::new(touch.position()));
    }
}

pub fn stop_player_throttling(
    mut commands: Commands,
    player_query: Query<Entity, (With<Player>, With<Throttling>)>,
) {
    for player_entity in player_query.iter() {
        commands.entity(player_entity).remove::<Throttling>();
    }
}
