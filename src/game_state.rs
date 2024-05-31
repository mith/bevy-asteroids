use bevy::prelude::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Default, States)]
pub enum GameState {
    #[default]
    Menu,
    Playing,
    Finished,
}

#[derive(Resource)]
pub enum GameResult {
    Win,
    Lose,
}
