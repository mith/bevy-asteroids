use bevy::{
    ecs::{
        component::Component,
        entity::Entity,
        query::With,
        system::{Commands, Query, Res},
    },
    hierarchy::DespawnRecursiveExt,
    math::Vec2,
    prelude::Resource,
    render::mesh::Mesh,
};
use bevy_rapier2d::{geometry::Collider, plugin::RapierContext};
use itertools::Itertools;

pub fn mesh_to_collider(mesh: &Mesh) -> Result<Collider, String> {
    let vertices = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .ok_or("Failed to get attribute position")?
        .as_float3()
        .ok_or("Failed to convert attribute to float3")?
        .iter()
        .map(|pos| Vec2::new(pos[0], pos[1])) // Ensure 2D is intended
        .collect::<Vec<_>>();

    let indices_vec = mesh
        .indices()
        .ok_or("Failed to get indices")?
        .iter()
        .map(|i| i as u32)
        .tuples()
        .map(|(i0, i1, i2)| [i0, i1, i2])
        .collect::<Vec<_>>();

    Ok(Collider::trimesh(vertices, indices_vec))
}

pub fn cleanup_component<T: Component>(mut commands: Commands, query: Query<Entity, With<T>>) {
    for entity in &query {
        commands.entity(entity).despawn_recursive();
    }
}

pub fn cleanup_resource<T: Resource>(mut commands: Commands) {
    commands.remove_resource::<T>();
}

pub fn contact_position_and_normal(
    rapier_context: &Res<RapierContext>,
    entity_a: Entity,
    entity_b: Entity,
) -> Option<(Vec2, Vec2)> {
    let contact = rapier_context.contact_pair(entity_a, entity_b)?;
    if !contact.has_any_active_contacts() {
        return None;
    }

    let (contact_manifold, contact_view) = contact.find_deepest_contact()?;

    Some((contact_manifold.normal(), contact_view.local_p2()))
}
