use bevy::{
    app::{App, Plugin, Update},
    asset::Assets,
    ecs::{
        component::Component,
        entity::Entity,
        event::{EventReader, EventWriter},
        query::With,
        schedule::{
            common_conditions::{in_state, not, resource_added, resource_exists_and_equals},
            Condition, IntoSystemConfigs, SystemSet,
        },
        system::{Commands, Local, Query, Res, ResMut, Resource},
    },
    input::{
        mouse::MouseButton,
        touch::{TouchInput, TouchPhase},
        ButtonInput,
    },
    math::{primitives::Circle, Quat, Rect, Vec2, Vec3},
    prelude::default,
    render::{camera::Camera, color::Color, mesh::Mesh},
    sprite::{ColorMaterial, MaterialMesh2dBundle},
    transform::components::{GlobalTransform, Transform},
    window::{PrimaryWindow, Window},
};

use crate::{
    ship::{Ship, Throttling},
    turret::FireEvent,
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
        )
        .add_systems(
            Update,
            spawn_shoot_button.run_if(
                in_state(GameState::Playing)
                    .and_then(resource_added::<InputMode>)
                    .and_then(resource_exists_and_equals(InputMode::Touch)),
            ),
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

#[derive(Default)]
struct TouchState(Option<Vec2>);

fn player_ship_touch_input(
    mut commands: Commands,
    mut touches: EventReader<TouchInput>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut player_query: Query<(Entity, &GlobalTransform, &mut Transform), (With<Player>, With<Ship>)>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    mut fire_projectile_event_writer: EventWriter<FireEvent>,
    mut touch_state: Local<TouchState>,
) {
    let (camera, camera_global_transform) = camera_query.single();
    let primary_window = windows.single();

    let button_ratio = 0.2;
    let button_size =
        (primary_window.height() * button_ratio).min(primary_window.width() * button_ratio);

    // Define the region for firing projectiles
    let fire_button_region = Rect {
        min: Vec2::new(
            primary_window.width() / 2. - button_size,
            -primary_window.height() / 2.,
        ), // Adjust size as needed
        max: Vec2::new(
            primary_window.width() / 2.,
            -primary_window.height() / 2. + button_size,
        ),
    };

    for touch in touches.read() {
        match touch.phase {
            TouchPhase::Started | TouchPhase::Moved => {
                *touch_state = TouchState(Some(touch.position));
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                *touch_state = TouchState(None);
            }
        }
    }

    if let Some(touch_pos) = touch_state.0 {
        // This could be a short tap or the start of a hold
        let touch_world_pos = camera
            .viewport_to_world_2d(camera_global_transform, touch_pos)
            .expect("Touch position not in world coordinates");

        if fire_button_region.contains(touch_world_pos) {
            // Touch is in the right bottom corner, fire a projectile
            for (player_entity, _, _) in player_query.iter() {
                fire_projectile_event_writer.send(FireEvent {
                    turret_entity: player_entity,
                });
            }
        } else {
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

#[derive(Component)]
struct ShootButton;

fn spawn_shoot_button(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let window = windows.single();
    let button_ratio = 0.2;
    let button_size = (window.height() * button_ratio).min(window.width() * button_ratio);
    let mesh_handle = meshes.add(Circle {
        radius: button_size / 2.,
    });

    commands.spawn((
        ShootButton,
        MaterialMesh2dBundle {
            mesh: mesh_handle.into(),
            material: materials.add(ColorMaterial::from(Color::RED)),
            transform: Transform::from_translation(Vec3::new(
                window.width() / 2. - button_size / 2.,
                -window.height() / 2. + button_size / 2.,
                10.,
            )),
            ..default()
        },
    ));
}
