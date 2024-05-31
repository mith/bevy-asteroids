use crate::{
    game_state::{GameResult, GameState},
    utils::cleanup,
};
use bevy::{
    app::{App, Plugin, Update},
    ecs::{
        schedule::{common_conditions::in_state, IntoSystemConfigs, NextState, OnEnter, OnExit},
        system::ResMut,
    },
    input::{keyboard::KeyCode, ButtonInput},
    log::info,
    prelude::{
        default, AlignItems, AssetServer, BuildChildren, Color, Commands, Component, FlexDirection,
        JustifyContent, Name, NodeBundle, Res, Style, TextBundle, TextStyle, Val,
    },
    ui::UiRect,
};

pub struct StartScreenPlugin;

impl Plugin for StartScreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Menu), spawn_start_screen)
            .add_systems(OnExit(GameState::Menu), cleanup::<StartScreen>)
            .add_systems(Update, start_game.run_if(in_state(GameState::Menu)));
    }
}

#[derive(Component)]
pub struct StartScreen;

const FONT_PATH: &str = "fonts/TurretRoad-ExtraLight.ttf";

pub fn spawn_start_screen(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Name::new("Start screen"),
            StartScreen,
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Name::new("Title text"),
                TextBundle::from_section(
                    "Asteroids",
                    TextStyle {
                        font: asset_server.load(FONT_PATH),
                        font_size: 90.,
                        color: Color::WHITE,
                    },
                ),
            ));

            parent
                .spawn((
                    Name::new("Instructions"),
                    NodeBundle {
                        style: Style {
                            margin: UiRect::all(Val::Px(20.)),
                            align_items: AlignItems::Center,
                            flex_direction: FlexDirection::Column,
                            ..default()
                        },
                        ..default()
                    },
                ))
                .with_children(|parent| {
                    let instruction_style = TextStyle {
                        font: asset_server.load(FONT_PATH),
                        font_size: 40.,
                        color: Color::WHITE,
                    };
                    parent.spawn(TextBundle::from_section(
                        "Point cursor to aim ship",
                        instruction_style.clone(),
                    ));
                    parent.spawn(TextBundle::from_section(
                        "Left click to fire thrusters",
                        instruction_style.clone(),
                    ));
                    parent.spawn(TextBundle::from_section(
                        "Right click to fire turret",
                        instruction_style.clone(),
                    ));
                });

            parent.spawn((
                Name::new("Continue instructions text"),
                TextBundle::from_section(
                    "Press Space to start",
                    TextStyle {
                        font: asset_server.load(FONT_PATH),
                        font_size: 40.,
                        color: Color::WHITE,
                    },
                ),
            ));
        });
}

fn start_game(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut next_gamestate: ResMut<NextState<GameState>>,
) {
    if keyboard_input.pressed(KeyCode::Space) {
        next_gamestate.set(GameState::Playing);
        info!("Starting game");
    }
}

pub struct FinishedScreenPlugin;

impl Plugin for FinishedScreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Finished), show_game_finished)
            .add_systems(OnExit(GameState::Finished), cleanup::<FinishedText>);
    }
}

#[derive(Component)]
pub struct FinishedText;

pub fn show_game_finished(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    game_result: Res<GameResult>,
) {
    commands
        .spawn((
            FinishedText,
            Name::new("Finished screen"),
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Name::new("Game result text"),
                TextBundle::from_section(
                    match *game_result {
                        GameResult::Win => "You win!",
                        GameResult::Lose => "Game over!",
                    },
                    TextStyle {
                        font: asset_server.load(FONT_PATH),
                        font_size: 90.,
                        color: Color::WHITE,
                    },
                ),
            ));
            parent.spawn((
                Name::new("Restart text"),
                TextBundle::from_section(
                    "Press R to restart",
                    TextStyle {
                        font: asset_server.load(FONT_PATH),
                        font_size: 40.,
                        color: Color::WHITE,
                    },
                ),
            ));
        });
}
