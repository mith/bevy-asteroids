use bevy::{
    app::{App, Plugin, Update},
    asset::{Assets, Handle},
    ecs::{
        component::Component,
        entity::Entity,
        event::{Event, EventReader},
        schedule::{apply_deferred, IntoSystemConfigs, SystemSet},
        system::{Commands, Query, Res, ResMut},
    },
    log::{error, info},
    math::{
        primitives::{Rectangle, RegularPolygon},
        Vec2, Vec3, Vec3Swizzles,
    },
    render::{
        color::Color,
        mesh::{Mesh, VertexAttributeValues},
    },
    sprite::{ColorMaterial, MaterialMesh2dBundle, Mesh2dHandle},
    time::{Time, Timer, TimerMode},
    transform::components::Transform,
    utils::default,
};
use bevy_rapier2d::{
    dynamics::{RigidBody, Sleeping, Velocity},
    geometry::{ActiveEvents, CollisionGroups, Group, Restitution},
};
use rand::{rngs::ThreadRng, Rng};

use crate::{
    edge_wrap::{Bounds, Duplicable},
    mesh_utils::calculate_mesh_area,
    split_mesh::{shatter_mesh, split_mesh, trim_mesh},
    utils::mesh_to_collider,
};

pub struct AsteroidPlugin;

impl Plugin for AsteroidPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<SplitAsteroidEvent>().add_systems(
            Update,
            (split_asteroid_event, apply_deferred, debris_lifetime)
                .chain()
                .in_set(AsteroidSet),
        );
    }
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct AsteroidSet;

#[derive(Component)]
pub struct Asteroid;

pub const ASTEROID_MAX_VERTICES: usize = 14;
pub const ASTEROID_MAX_VERTICE_DRIFT: f32 = 8.;
pub const ASTEROID_MAX_SPAWN_LIN_VELOCITY: f32 = 50.;
pub const ASTEROID_MAX_SPAWN_ANG_VELOCITY: f32 = 1.;
const ASTEROID_SPAWN_CIRCUMRADIUS: f32 = 50.;

pub const RECTANGULAR_ASTEROIDS: bool = false;
pub const FROZEN_ASTEROIDS: bool = false;

pub fn spawn_asteroids(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    bounds: Res<Bounds>,
) {
    // Divide bounds area by approximate asteroid area to get a rough estimate of how many asteroids to spawn
    let asteroid_spawn_count = (((bounds.0.x * bounds.0.y) as usize
        / (ASTEROID_SPAWN_CIRCUMRADIUS * ASTEROID_SPAWN_CIRCUMRADIUS) as usize)
        / 10)
        .clamp(2, 10);
    info!(bounds= ?bounds, number= ?asteroid_spawn_count, "Spawning asteroids");
    let mut asteroid_positions: Vec<Vec2> = Vec::new();
    while asteroid_positions.len() < asteroid_spawn_count {
        let mut rng = rand::thread_rng();
        let max_x = bounds.0.x;
        let max_y = bounds.0.y;
        let asteroid_pos = Vec2::new(rng.gen_range(-max_x..max_x), rng.gen_range(-max_y..max_y));

        // skip spawning asteroids on top of player
        if asteroid_pos.length() < 150. {
            continue;
        }

        // skip spawning asteroids on top of other asteroids
        if asteroid_positions
            .iter()
            .any(|&pos| (pos - asteroid_pos).length() < 150.)
        {
            continue;
        }

        asteroid_positions.push(asteroid_pos);

        spawn_asteroid(
            rng,
            &mut commands,
            asteroid_pos,
            &mut meshes,
            &mut materials,
        );
    }
}

pub const ASTEROID_GROUP: Group = Group::GROUP_3;

pub fn spawn_asteroid(
    mut rng: rand::prelude::ThreadRng,
    commands: &mut Commands,
    asteroid_pos: Vec2,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) {
    let asteroid_velocity = Vec2::new(
        rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY),
        rng.gen_range(-ASTEROID_MAX_SPAWN_LIN_VELOCITY..ASTEROID_MAX_SPAWN_LIN_VELOCITY),
    );
    let asteroid_angular_velocity =
        rng.gen_range(-ASTEROID_MAX_SPAWN_ANG_VELOCITY..ASTEROID_MAX_SPAWN_ANG_VELOCITY);

    let mut asteroid_mesh = if !RECTANGULAR_ASTEROIDS {
        Mesh::from(RegularPolygon::new(
            ASTEROID_SPAWN_CIRCUMRADIUS,
            ASTEROID_MAX_VERTICES,
        ))
    } else {
        Mesh::from(Rectangle::new(100., 100.))
    };

    if !RECTANGULAR_ASTEROIDS {
        let pos_attributes = asteroid_mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION).expect(
        "Mesh does not have a position attribute. This should not happen as we just created the mesh",
        );

        let VertexAttributeValues::Float32x3(pos_attr_vec3) = pos_attributes else {
            panic!("Position attribute is not a Float32x3");
        };

        pos_attr_vec3.iter_mut().for_each(|v| {
            // Translate vertice randomly
            v[0] += rng.gen_range(-ASTEROID_MAX_VERTICE_DRIFT..ASTEROID_MAX_VERTICE_DRIFT);
            v[1] += rng.gen_range(-ASTEROID_MAX_VERTICE_DRIFT..ASTEROID_MAX_VERTICE_DRIFT);
        });
    }

    let collider = mesh_to_collider(&asteroid_mesh);

    let mut asteroid_cmd = commands.spawn((
        Asteroid,
        MaterialMesh2dBundle {
            transform: Transform::default().with_translation(Vec3::new(
                asteroid_pos.x,
                asteroid_pos.y,
                0.,
            )),
            mesh: meshes.add(asteroid_mesh).into(),
            material: materials.add(ColorMaterial::from(Color::WHITE)),
            ..default()
        },
        collider,
        ActiveEvents::COLLISION_EVENTS,
        Duplicable,
        CollisionGroups::new(ASTEROID_GROUP, Group::ALL),
    ));

    if !FROZEN_ASTEROIDS {
        asteroid_cmd.insert((
            RigidBody::Dynamic,
            Velocity {
                linvel: asteroid_velocity,
                angvel: asteroid_angular_velocity,
            },
            Restitution {
                coefficient: 0.9,
                ..default()
            },
            Sleeping {
                normalized_linear_threshold: 0.001,
                angular_threshold: 0.001,
                ..default()
            },
        ));
    } else {
        asteroid_cmd.insert(RigidBody::Fixed);
    }
}

#[derive(Event)]
pub struct SplitAsteroidEvent {
    pub asteroid_entity: Entity,
    pub collision_direction: Vec2,
    pub collision_position: Vec2,
}

#[derive(Component)]
pub struct Debris {
    lifetime: Timer,
}

const ASTEROID_MIN_AREA: f32 = 500.;

const DEBRIS_MAX_AREA: f32 = 6.;

pub fn split_asteroid_event(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut asteroid_query: Query<(&Transform, &Velocity, &mut Mesh2dHandle)>,
    mut split_asteroid_events: EventReader<SplitAsteroidEvent>,
) {
    for event in split_asteroid_events.read() {
        let (transform, velocity, mesh_handle) = asteroid_query
            .get_mut(event.asteroid_entity)
            .expect("Asteroid entity not found");
        split_asteroid(
            &mut commands,
            &mesh_handle.0,
            &mut meshes,
            &mut materials,
            transform,
            *velocity,
            event.collision_direction,
            event.collision_position,
        );

        commands.entity(event.asteroid_entity).despawn();
    }
}

fn split_asteroid(
    commands: &mut Commands,
    original_mesh: &Handle<Mesh>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    transform: &Transform,
    velocity: Velocity,
    collision_direction: Vec2,
    collision_position: Vec2,
) {
    let mesh = meshes.get(original_mesh).expect("Original mesh not found");

    // Rotate the collision direction by the rotation of the asteroid
    // to get the collision direction in the asteroid's local space.
    let asteroid_rotation = transform.rotation;
    let mesh_collision_direction = asteroid_rotation
        .inverse()
        .mul_vec3(collision_direction.extend(0.))
        .truncate()
        .normalize();

    let [(mesh_a, offset_a), (mesh_b, offset_b)] =
        split_mesh(mesh, mesh_collision_direction, collision_position);

    // Calculate area of the split mesh
    // Skip spawning if the area of the split mesh is too small

    let mut spawn = |mesh: &Mesh, offset: Vec2| {
        // Check if mesh contains any vertices
        if mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .map(|attr| attr.len() < 3)
            .unwrap_or(true)
        {
            error!("Mesh has no triangles, skipping split");
            return;
        }
        let trimmed = trim_mesh(mesh);
        let translation = transform.transform_point((offset + trimmed.0 .1).extend(0.));
        let main_transform =
            Transform::from_translation(translation).with_rotation(transform.rotation);
        let velocity = Velocity {
            linvel: velocity.linvel
                + asteroid_rotation
                    .mul_vec3(offset.extend(0.))
                    .truncate()
                    .normalize()
                    * 50.,
            angvel: velocity.angvel,
        };
        let mesh_area = calculate_mesh_area(mesh);
        if mesh_area > ASTEROID_MIN_AREA {
            // let mesh = round_mesh(mesh).0;
            spawn_asteroid_split(
                commands,
                main_transform,
                velocity,
                meshes,
                materials,
                &trimmed.0 .0,
            );
        } else if mesh_area > 0. && mesh_area < ASTEROID_MIN_AREA {
            spawn_shattered_mesh(mesh, &main_transform, velocity, commands, meshes, materials);
        }

        for (mesh, trimmed_offset) in trimmed.1 {
            let translation = transform.transform_point((offset + trimmed_offset).extend(0.));
            let transform =
                Transform::from_translation(translation).with_rotation(main_transform.rotation);
            spawn_shattered_mesh(&mesh, &transform, velocity, commands, meshes, materials)
        }
    };

    spawn(&mesh_a, offset_a);
    spawn(&mesh_b, offset_b);
}

pub fn spawn_shattered_mesh(
    mesh: &Mesh,
    transform: &Transform,
    velocity: Velocity,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) {
    let mut rng = ThreadRng::default();
    let shards = shatter_mesh(mesh, DEBRIS_MAX_AREA);
    for (mesh, offset) in shards.iter() {
        let shard_translation = transform.transform_point(offset.extend(0.));
        let shard_transform =
            Transform::from_translation(shard_translation).with_rotation(transform.rotation);

        let rng_range_max = 5.;

        let velocity = Velocity {
            linvel: transform
                .rotation
                .mul_vec3(offset.extend(0.))
                .normalize()
                .xy()
                * 15.
                + velocity.linvel
                + Vec2::new(
                    rng.gen_range(-rng_range_max..rng_range_max),
                    rng.gen_range(-rng_range_max..rng_range_max),
                ),
            angvel: rng
                .gen_range(-ASTEROID_MAX_SPAWN_ANG_VELOCITY..ASTEROID_MAX_SPAWN_ANG_VELOCITY),
        };

        spawn_debris(
            commands,
            &shard_transform,
            velocity,
            meshes,
            materials,
            mesh,
        )
    }
}

pub fn spawn_asteroid_split(
    commands: &mut Commands,
    transform: Transform,
    velocity: Velocity,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    mesh: &Mesh,
) {
    let collider = mesh_to_collider(mesh);

    let mut asteroid_cmd = commands.spawn((
        Asteroid,
        MaterialMesh2dBundle {
            transform,
            mesh: Mesh2dHandle(meshes.add(mesh.clone())),
            material: materials.add(ColorMaterial::from(Color::WHITE)),
            ..default()
        },
        collider,
        ActiveEvents::COLLISION_EVENTS,
        Duplicable,
    ));
    if !FROZEN_ASTEROIDS {
        asteroid_cmd.insert((
            RigidBody::Dynamic,
            velocity,
            Restitution {
                coefficient: 0.9,
                ..default()
            },
            Sleeping {
                normalized_linear_threshold: 0.001,
                angular_threshold: 0.001,
                ..default()
            },
        ));
    } else {
        asteroid_cmd.insert(RigidBody::Fixed);
    }
}

pub const DEBRIS_GROUP: Group = Group::GROUP_4;

pub fn spawn_debris(
    commands: &mut Commands,
    transform: &Transform,
    velocity: Velocity,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    mesh: &Mesh,
) {
    let collider = mesh_to_collider(mesh);
    let mut rng = ThreadRng::default();

    let mut asteroid_cmd = commands.spawn((
        Debris {
            lifetime: Timer::from_seconds(rng.gen_range(0.5..5.0), TimerMode::Once),
        },
        MaterialMesh2dBundle {
            transform: *transform,
            mesh: Mesh2dHandle(meshes.add(mesh.clone())),
            material: materials.add(ColorMaterial::from(Color::WHITE)),
            ..default()
        },
        collider,
        CollisionGroups::new(DEBRIS_GROUP, Group::NONE),
        Duplicable,
    ));
    if !FROZEN_ASTEROIDS {
        asteroid_cmd.insert((
            RigidBody::Dynamic,
            velocity,
            Restitution {
                coefficient: 0.9,
                ..default()
            },
            Sleeping {
                normalized_linear_threshold: 0.001,
                angular_threshold: 0.001,
                ..default()
            },
        ));
    } else {
        asteroid_cmd.insert(RigidBody::Fixed);
    }
}

pub fn debris_lifetime(
    mut commands: Commands,
    time: Res<Time>,
    mut debris_query: Query<(Entity, &mut Debris)>,
) {
    for (entity, mut debris) in &mut debris_query {
        debris.lifetime.tick(time.delta());
        if debris.lifetime.finished() {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {

    use bevy::{
        app::{App, Startup},
        math::{primitives::Rectangle, Quat},
    };

    use crate::asteroid::split_asteroid;

    use super::*;

    #[test]
    fn test_split_asteroid_rectangle() {
        let mut app = App::new();

        app.insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<ColorMaterial>::default());

        app.add_systems(
            Startup,
            |mut commands: Commands,
             mut meshes: ResMut<Assets<Mesh>>,
             mut materials: ResMut<Assets<ColorMaterial>>| {
                let rectangle_shape = Rectangle::from_size(Vec2::new(100., 100.));
                let asteroid_mesh = Mesh::from(rectangle_shape);
                let mesh_handle = meshes.add(asteroid_mesh.clone());

                let transform = Transform::default();

                split_asteroid(
                    &mut commands,
                    &mesh_handle,
                    &mut meshes,
                    &mut materials,
                    &transform,
                    Velocity::zero(),
                    Vec2::new(0., 1.),
                    Vec2::ZERO,
                );
            },
        );

        app.run();

        // Check that 2 splits were created
        // They should be located at (-25, 0) and (25, 0)

        assert_eq!(app.world.query::<&Asteroid>().iter(&app.world).len(), 2);

        app.world
            .query::<(&Transform, &Asteroid)>()
            .iter(&app.world)
            .for_each(|(transform, _)| {
                let translation = transform.translation;
                assert!(translation.x == -25. || translation.x == 25.);
                assert_eq!(translation.y, 0.);
            });
    }

    #[test]
    fn test_split_asteroid_rectangle_90_cw_rotated() {
        let mut app = App::new();

        app.insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<ColorMaterial>::default());

        app.add_systems(
            Startup,
            |mut commands: Commands,
             mut meshes: ResMut<Assets<Mesh>>,
             mut materials: ResMut<Assets<ColorMaterial>>| {
                let rectangle_shape = Rectangle::from_size(Vec2::new(100., 100.));
                let asteroid_mesh = Mesh::from(rectangle_shape);
                let mesh_handle = meshes.add(asteroid_mesh.clone());

                let transform =
                    Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));

                split_asteroid(
                    &mut commands,
                    &mesh_handle,
                    &mut meshes,
                    &mut materials,
                    &transform,
                    Velocity::zero(),
                    Vec2::new(0., 1.),
                    Vec2::ZERO,
                );
            },
        );

        app.run();

        // Check that 2 splits were created
        // They should be located at (0, -25) and (0, 25)

        assert_eq!(app.world.query::<&Asteroid>().iter(&app.world).len(), 2);

        app.world
            .query::<(&Transform, &Asteroid)>()
            .iter(&app.world)
            .for_each(|(transform, _)| {
                let translation = transform.translation;
                assert!(translation.y == -25. || translation.y == 25.);
                assert_eq!(translation.x, 0.);
            });
    }
}
