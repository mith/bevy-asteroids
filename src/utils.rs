use bevy::{
    ecs::{
        component::Component,
        entity::Entity,
        query::With,
        system::{Commands, Query},
    },
    hierarchy::DespawnRecursiveExt,
    math::Vec2,
    render::mesh::Mesh,
};
use bevy_rapier2d::geometry::Collider;

pub fn mesh_to_collider(mesh: &Mesh) -> Collider {
    let vertices = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .unwrap()
        .as_float3()
        .unwrap()
        .to_vec()
        .iter()
        .map(|pos| Vec2::new(pos[0], pos[1]))
        .collect::<_>();
    let indices_vec = mesh
        .indices()
        .unwrap()
        .iter()
        .map(|i| i as u32)
        .collect::<Vec<u32>>()
        .chunks(3)
        .map(|chunk| [chunk[0], chunk[1], chunk[2]])
        .collect::<_>();
    Collider::trimesh(vertices, indices_vec)
}
pub fn cleanup<T: Component>(mut commands: Commands, query: Query<Entity, With<T>>) {
    for entity in &query {
        commands.entity(entity).despawn_recursive();
    }
}
