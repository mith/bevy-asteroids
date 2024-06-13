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
    math::Quat,
    render::{camera::Camera, color::Color},
    transform::components::{GlobalTransform, Transform},
    ui::{BackgroundColor, Interaction, Node},
    window::{PrimaryWindow, Window},
};

use crate::{
    ship::{Ship, Throttling},
    turret::FireEvent,
    ui::ShootButton,
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
                    player_ship_touch_input.run_if(resource_exists_and_equals(InputMode::Touch)),
                )
                    .run_if(in_state(GameState::Playing)),
                stop_player_throttling.run_if(not(in_state(GameState::Playing))),
            )
                .in_set(PlayerInputSet),
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

fn player_ship_touch_input(
    mut commands: Commands,
    touches: Res<Touches>,
    mut shoot_button_query: Query<
        (&Interaction, &mut BackgroundColor, &GlobalTransform, &Node),
        With<ShootButton>,
    >,
    mut player_query: Query<(Entity, &GlobalTransform, &mut Transform), (With<Player>, With<Ship>)>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    mut fire_projectile_event_writer: EventWriter<FireEvent>,
) {
    let (camera, camera_global_transform) = camera_query.single();

    let (interaction, mut btn_bg_color, btn_transform, btn_node) = shoot_button_query.single_mut();

    if *interaction == Interaction::Pressed {
        for (player_entity, _, _) in player_query.iter() {
            fire_projectile_event_writer.send(FireEvent {
                turret_entity: player_entity,
            });
        }
        btn_bg_color.0 = Color::WHITE;
    } else {
        btn_bg_color.0 = Color::BLACK;
    }

    let button_rect = btn_node.logical_rect(btn_transform);

    if let Some(touch) = touches
        .iter()
        .find(|touch| !button_rect.contains(touch.position()))
    {
        let touch_world_pos = camera
            .viewport_to_world_2d(camera_global_transform, touch.position())
            .expect("Touch position not in world coordinates");
        {
            // Point ship towards touch location
            for (_, player_global_transform, mut player_transform) in player_query.iter_mut() {
                let direction = touch_world_pos - player_global_transform.translation().truncate();
                let angle = direction.y.atan2(direction.x);
                let target_rotation = Quat::from_rotation_z(angle - std::f32::consts::FRAC_PI_2);
                player_transform.rotation = target_rotation;
            }

            for (player_entity, _, _) in player_query.iter() {
                commands.entity(player_entity).insert(Throttling);
            }
        }
    } else {
        // Touch ended or cancelled, stop throttling
        for (player_entity, _, _) in player_query.iter() {
            commands.entity(player_entity).remove::<Throttling>();
        }
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
