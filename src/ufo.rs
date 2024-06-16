use bevy::{
    app::{App, Plugin, Startup, Update},
    asset::{AssetServer, Assets, Handle},
    ecs::{
        component::Component,
        entity::Entity,
        event::{Event, EventReader, EventWriter},
        query::With,
        schedule::{common_conditions::in_state, IntoSystemConfigs, OnEnter, SystemSet},
        system::{Commands, Query, Res, ResMut, Resource},
    },
    gizmos::{self, gizmos::Gizmos},
    hierarchy::DespawnRecursiveExt,
    math::{Quat, Rect, Vec2, Vec3, Vec3Swizzles},
    prelude::default,
    render::{color::Color, mesh::Mesh},
    sprite::{ColorMaterial, MaterialMesh2dBundle},
    time::{Time, Timer, TimerMode},
    transform::components::{GlobalTransform, Transform},
};
use bevy_rapier2d::{
    dynamics::{ExternalImpulse, LockedAxes, ReadMassProperties, RigidBody, Velocity},
    geometry::{Collider, CollisionGroups, Group, ShapeCastOptions},
    pipeline::QueryFilter,
    plugin::RapierContext,
};
use rand::Rng;
use tracing::info;

use crate::{
    asteroid::{Asteroid, SplitAsteroidEvent, ASTEROID_GROUP},
    edge_wrap::{Bounds, Duplicable},
    explosion,
    game_state::GameState,
    player::Player,
    projectile::PROJECTILE_GROUP,
    shatter::spawn_shattered_mesh,
    utils::mesh_to_collider,
};

pub struct UfoPlugin;

impl Plugin for UfoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_ufo_assets)
            // .add_systems(OnEnter(GameState::Playing), spawn_ufo)
            .add_event::<UfoDestroyedEvent>()
            .init_resource::<SpawnTimer>()
            .add_systems(OnEnter(GameState::Playing), reset_spawn_timer)
            .add_systems(
                Update,
                (
                    move_ufo,
                    ufo_inside_bounds,
                    throw_asteroid.run_if(in_state(GameState::Playing)),
                    ufo_destroyed,
                    spawn_ufo.run_if(in_state(GameState::Playing)),
                )
                    .chain(),
            );
    }
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct UfoSet;

#[derive(Component)]
pub struct Ufo;

#[derive(Resource)]
struct UfoAssets {
    ufo_mesh: Handle<Mesh>,
    ufo_material: Handle<ColorMaterial>,
}

fn load_ufo_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.insert_resource(UfoAssets {
        ufo_mesh: asset_server.load("meshes/ufo.glb#Mesh0/Primitive0"),
        ufo_material: materials.add(ColorMaterial::from(Color::WHITE)),
    });
}

pub const UFO_GROUP: Group = Group::GROUP_5;

#[derive(Resource)]
struct SpawnTimer {
    timer: Timer,
}

impl Default for SpawnTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(6., TimerMode::Once),
        }
    }
}

fn reset_spawn_timer(mut spawn_timer: ResMut<SpawnTimer>) {
    spawn_timer.timer.reset();
}

fn spawn_ufo(
    mut commands: Commands,
    ufo_assets: Res<UfoAssets>,
    meshes: Res<Assets<Mesh>>,
    ufo_query: Query<Entity, With<Ufo>>,
    mut split_asteroid_events: EventReader<SplitAsteroidEvent>,
    bounds: Res<Bounds>,
    mut spawn_timer: ResMut<SpawnTimer>,
    time: Res<Time>,
) {
    if !ufo_query.is_empty() {
        return;
    }

    if !spawn_timer.timer.tick(time.delta()).finished() {
        return;
    }

    let mut rng = rand::thread_rng();

    for _event in split_asteroid_events.read() {
        if rng.gen_bool(0.3) {
            continue;
        }
        info!("Spawning UFO");
        let mesh = meshes
            .get(&ufo_assets.ufo_mesh)
            .expect("Failed to load mesh");
        let collider = mesh_to_collider(mesh).expect("Failed to create collider");
        let direction = Quat::from_rotation_z(rng.gen_range(0.0..std::f32::consts::PI * 2.));
        let spawn_distance = Vec3::new(bounds.0.x * 2., bounds.0.y * 2., 0.);
        let translation = direction.mul_vec3(spawn_distance);
        commands.spawn((
            Ufo,
            MaterialMesh2dBundle {
                mesh: ufo_assets.ufo_mesh.clone().into(),
                material: ufo_assets.ufo_material.clone(),
                transform: Transform::from_translation(translation),
                ..default()
            },
            collider,
            CollisionGroups::new(UFO_GROUP, PROJECTILE_GROUP),
            RigidBody::KinematicVelocityBased,
            LockedAxes::ROTATION_LOCKED,
            TractorBeam::default(),
        ));
        return;
    }
}

const MAX_UFO_ACCELERATION: Vec2 = Vec2::splat(1000.);
const MAX_UFO_VELOCITY: f32 = 400.;

fn move_ufo(
    mut commands: Commands,
    mut ufo_query: Query<(Entity, &GlobalTransform, Option<&Velocity>, &Collider), With<Ufo>>,
    player_query: Query<&GlobalTransform, With<Player>>,
    collider_query: Query<(&GlobalTransform, Option<&Velocity>, &Collider)>,
    time: Res<Time>,
    rapier_context: Res<RapierContext>,
    mut gizmos: Gizmos,
) {
    let mut rng = rand::thread_rng();

    for (ufo_entity, ufo_transform, opt_ufo_velocity, ufo_collider) in ufo_query.iter_mut() {
        let player_impulse_strength =
            calculate_player_impulse(&player_query, ufo_transform, &mut rng);

        // Check for nearby obstacles to avoid
        let avoidance_impulse_strength = calculate_avoidance_impulse(
            &rapier_context,
            ufo_entity,
            ufo_transform,
            opt_ufo_velocity.unwrap_or(&Velocity::zero()),
            ufo_collider,
            &collider_query,
            &mut gizmos,
        );

        let dampen_impulse = if avoidance_impulse_strength.length() < 10. {
            -opt_ufo_velocity.map_or(Vec2::ZERO, |velocity| velocity.linvel) * 0.001
        } else {
            Vec2::ZERO
        };

        let impulse_strength =
            player_impulse_strength + avoidance_impulse_strength + dampen_impulse;

        let old_velocity = opt_ufo_velocity.map_or(Vec2::ZERO, |velocity| velocity.linvel);
        let new_velocity = impulse_strength * 50_000. * time.delta_seconds();

        let new_velocity = new_velocity.clamp(
            old_velocity - MAX_UFO_ACCELERATION * time.delta_seconds(),
            old_velocity + MAX_UFO_ACCELERATION * time.delta_seconds(),
        );

        let velocity = (old_velocity * 2. + new_velocity) / 3.;

        // Apply the final impulse
        commands.entity(ufo_entity).insert(Velocity {
            linvel: velocity.clamp(
                Vec2::splat(-MAX_UFO_VELOCITY),
                Vec2::splat(MAX_UFO_VELOCITY),
            ),
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
    gizmos: &mut Gizmos,
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
        let weight = ufo_velocity.linvel.length() + 1. / asteroid_ufo_distance.length();

        // Adjust direction based on which side of the normal the UFO is on
        let avoidance_impulse = if dot_product > 0. { -normal } else { normal } * weight;
        let start = ufo_transform.translation().xy();
        gizmos.line_2d(start, start + avoidance_impulse, Color::ORANGE);
        gizmos.circle_2d(asteroid_transform.translation().xy(), 40., Color::ORANGE);
        avoid_direction += avoidance_impulse;
    }

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

    if !intersections.is_empty() {
        for intersection_entity in intersections {
            let (asteroid_transform, opt_asteroid_velocity, asteroid_collider) = collider_query
                .get(intersection_entity)
                .expect("Asteroid collider not found");

            let asteroid_ufo_distance =
                asteroid_transform.translation().xy() - ufo_transform.translation().xy();

            if let Some(asteroid_velocity) = opt_asteroid_velocity {
                if rapier_context
                    .cast_shape(
                        asteroid_transform.translation().xy(),
                        0.,
                        asteroid_velocity.linvel,
                        asteroid_collider,
                        ShapeCastOptions {
                            max_time_of_impact: asteroid_ufo_distance.length(),
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

                    let weight = asteroid_velocity.linvel.length();

                    // Adjust direction based on which side of the normal the UFO is on
                    let avoidance_impulse =
                        if dot_product > 0. { -normal } else { normal } * weight;
                    let start = ufo_transform.translation().xy();
                    gizmos.line_2d(start, start + avoidance_impulse, Color::RED);
                    gizmos.circle_2d(asteroid_transform.translation().xy(), 30., Color::RED);
                    avoid_direction += avoidance_impulse;
                };
            }

            let weight = 1. / asteroid_ufo_distance.length().powi(3);
            let asteroid_ufo_direction = asteroid_ufo_distance.normalize();
            let avoidance_impulse = -asteroid_ufo_direction * weight * 50000000.;
            let start = ufo_transform.translation().xy();
            gizmos.line_2d(start, start + avoidance_impulse, Color::GREEN);
            gizmos.circle_2d(asteroid_transform.translation().xy(), 20., Color::GREEN);
            avoid_direction += avoidance_impulse;
        }
    }

    avoid_direction
}

fn calculate_player_impulse(
    player_query: &Query<&GlobalTransform, With<Player>>,
    ufo_transform: &GlobalTransform,
    rng: &mut rand::prelude::ThreadRng,
) -> Vec2 {
    if let Ok(player_transform) = player_query.get_single() {
        let player_xy_distance =
            (player_transform.translation() - ufo_transform.translation()).xy();
        let player_distance = player_xy_distance.length();

        // Calculate impulse strength based on distance to the player
        let min_distance = 400.0;
        let max_distance = 600.;

        if player_distance < 1.0 {
            Vec2::new(rng.gen_range(-1.0..1.0), rng.gen_range(-1.0..1.0))
        } else if player_distance < min_distance {
            -player_xy_distance.normalize()
        } else if player_distance > max_distance {
            player_xy_distance.normalize()
        } else {
            let ufo_translation = ufo_transform.translation().xy();
            -ufo_translation.normalize_or_zero() * 00.1
        }
    } else {
        Vec2::ZERO
    }
}

#[derive(Component)]
struct InsideBounds;

fn ufo_inside_bounds(
    mut commands: Commands,
    ufo_query: Query<(Entity, &GlobalTransform), With<Ufo>>,
    bounds: Res<Bounds>,
) {
    let bounds_rect = Rect::from_center_half_size(Vec2::ZERO, bounds.0);
    for (ufo_entity, ufo_transform) in ufo_query.iter() {
        if bounds_rect.contains(ufo_transform.translation().xy()) {
            commands
                .entity(ufo_entity)
                .insert((InsideBounds, Duplicable));
        } else {
            commands.entity(ufo_entity).remove::<InsideBounds>();
        }
    }
}

const TRACTOR_BEAM_RELOAD_TIME: f32 = 4.;
const TRACTOR_BEAM_ARMED_TIME: f32 = 2.;
const TRACTOR_BEAM_FORCE: f32 = 250000.;

enum TractorBeamState {
    Armed(Timer),
    Reloading(Timer),
}

#[derive(Component)]
struct TractorBeam {
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

fn throw_asteroid(
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

#[derive(Event)]
pub struct UfoDestroyedEvent {
    pub ufo_entity: Entity,
}

fn ufo_destroyed(
    mut commands: Commands,
    mut ufo_destroyed_events: EventReader<UfoDestroyedEvent>,
    mut meshes: ResMut<Assets<Mesh>>,
    ufo_assets: Res<UfoAssets>,
    ufo_query: Query<(&Transform, Option<&Velocity>)>,
    mut spawn_timer: ResMut<SpawnTimer>,
    mut explosion_events: EventWriter<explosion::ExplosionEvent>,
) {
    for UfoDestroyedEvent { ufo_entity } in ufo_destroyed_events.read() {
        let mesh = meshes
            .get(&ufo_assets.ufo_mesh)
            .expect("Failed to load mesh")
            .clone();

        let (ufo_transform, opt_ufo_velocity) = ufo_query.get(*ufo_entity).expect("UFO not found");

        spawn_shattered_mesh(
            &mesh,
            ufo_assets.ufo_material.clone(),
            ufo_transform,
            opt_ufo_velocity.copied().unwrap_or(Velocity::zero()),
            &mut commands,
            &mut meshes,
        );

        commands.entity(*ufo_entity).despawn_recursive();

        explosion_events.send(explosion::ExplosionEvent {
            position: ufo_transform.translation.xy(),
            radius: 15.,
        });

        spawn_timer.timer.reset();
    }
}
