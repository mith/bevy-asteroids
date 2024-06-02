use bevy::{
    core::Name,
    ecs::{component::Component, system::Commands},
    transform::components::Transform,
};

use crate::ship::SpawnShipExt;

#[derive(Component)]
pub struct Player;

pub fn spawn_player(mut commands: Commands) {
    {
        let transform = Transform::default();
        let mut ship_cmd = commands.spawn_ship(transform);
        // let ship_entity = spawn_ship(commands, meshes, materials, transform);
        ship_cmd.insert((Name::new("Player"), Player));
    };
}
