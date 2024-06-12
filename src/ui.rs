use crate::{
    game_state::{GameResult, GameState},
    input::InputMode,
    utils::cleanup,
};
use bevy::{
    app::{App, Plugin, Update},
    ecs::{
        entity::Entity,
        event::EventReader,
        query::With,
        schedule::{
            common_conditions::in_state, Condition, IntoSystemConfigs, NextState, OnEnter, OnExit,
            States,
        },
        system::{Query, ResMut},
    },
    input::{
        mouse::MouseButton,
        touch::{TouchInput, TouchPhase, Touches},
        ButtonInput,
    },
    log::info,
    prelude::{
        default, AlignItems, AssetServer, BuildChildren, Color, Commands, Component, FlexDirection,
        JustifyContent, Name, NodeBundle, Res, Style, TextBundle, TextStyle, Val,
    },
    time::{Time, Timer, TimerMode},
    ui::UiRect,
};

pub struct StartScreenPlugin;

impl Plugin for StartScreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Menu), spawn_start_screen)
            .add_systems(OnExit(GameState::Menu), cleanup::<StartScreen>)
            .init_state::<StartScreenState>()
            .add_systems(OnExit(StartScreenState::Start), cleanup::<ClickOrTap>)
            .add_systems(
                Update,
                set_input_mode
                    .run_if(in_state(GameState::Menu).and_then(in_state(StartScreenState::Start))),
            )
            .add_systems(OnEnter(StartScreenState::Instructions), spawn_instructions)
            .add_systems(
                Update,
                start_game.run_if(
                    in_state(GameState::Menu).and_then(in_state(StartScreenState::Instructions)),
                ),
            )
            .add_systems(
                OnExit(StartScreenState::Instructions),
                cleanup::<Instructions>,
            );
    }
}

#[derive(Component)]
pub struct StartScreen;

#[derive(Component)]
struct ClickOrTap;

#[derive(Component)]
struct Instructions;

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum StartScreenState {
    #[default]
    Start,
    Instructions,
}

const FONT_PATH: &str = "fonts/TurretRoad-ExtraLight.ttf";

fn spawn_start_screen(mut commands: Commands, asset_server: Res<AssetServer>) {
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

            spawn_click_or_tap(parent, &asset_server);
        });
}

fn spawn_click_or_tap(parent: &mut bevy::prelude::ChildBuilder, asset_server: &AssetServer) {
    parent.spawn((
        Name::new("Click or tap text"),
        ClickOrTap,
        TextBundle::from_section(
            "Click or tap to continue",
            TextStyle {
                font: asset_server.load(FONT_PATH),
                font_size: 40.,
                color: Color::WHITE,
            },
        ),
    ));
}

fn set_input_mode(
    mut commands: Commands,
    mouse_input: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    mut next_start_screen_state: ResMut<NextState<StartScreenState>>,
) {
    if mouse_input.just_pressed(MouseButton::Left) {
        commands.insert_resource(InputMode::Mouse);
        next_start_screen_state.set(StartScreenState::Instructions);
    }

    if touches.iter().next().is_some() {
        commands.insert_resource(InputMode::Touch);
        next_start_screen_state.set(StartScreenState::Instructions);
    }
}

fn spawn_instructions(
    mut commands: Commands,
    start_screen_query: Query<Entity, With<StartScreen>>,
    asset_server: Res<AssetServer>,
    input_mode: Res<InputMode>,
) {
    let start_screen = start_screen_query.single();
    commands.entity(start_screen).with_children(|parent| {
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
                    match *input_mode {
                        InputMode::Mouse => "Point cursor to aim ship",
                        InputMode::Touch => "Touch to aim ship",
                    },
                    instruction_style.clone(),
                ));
                parent.spawn(TextBundle::from_section(
                    match *input_mode {
                        InputMode::Mouse => "Hold click to fire thrusters",
                        InputMode::Touch => "Hold touch to fire thrusters",
                    },
                    instruction_style.clone(),
                ));
                parent.spawn(TextBundle::from_section(
                    match *input_mode {
                        InputMode::Mouse => "Right click to fire turret",
                        InputMode::Touch => "Tap right bottom corner to fire turret",
                    },
                    instruction_style.clone(),
                ));
            });

        parent.spawn((
            Name::new("Continue instructions text"),
            TextBundle::from_section(
                match *input_mode {
                    InputMode::Mouse => "Click anywhere to start",
                    InputMode::Touch => "Tap anywhere to start",
                },
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
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut touch_events: EventReader<TouchInput>,
    mut next_gamestate: ResMut<NextState<GameState>>,
) {
    if mouse_input.just_pressed(MouseButton::Left)
        || touch_events.read().any(|t| t.phase == TouchPhase::Started)
    {
        next_gamestate.set(GameState::Playing);
        info!("Starting game");
    }
}

pub struct FinishedScreenPlugin;

impl Plugin for FinishedScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<FinishedScreenState>()
            .add_systems(OnEnter(GameState::Finished), spawn_game_finished_screen)
            .add_systems(OnExit(GameState::Finished), cleanup::<FinishedText>)
            .add_systems(
                Update,
                (
                    finished_screen_timer,
                    restart_game.run_if(in_state(FinishedScreenState::PromptRestart)),
                )
                    .run_if(in_state(GameState::Finished)),
            );
    }
}

#[derive(Component)]
pub struct FinishedText {
    timer: Timer,
}

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FinishedScreenState {
    #[default]
    Locked,
    PromptRestart,
}

impl Default for FinishedText {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(3.0, TimerMode::Once),
        }
    }
}

fn spawn_game_finished_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    game_result: Res<GameResult>,
    mut next_finished_screen_state: ResMut<NextState<FinishedScreenState>>,
) {
    commands
        .spawn((
            FinishedText::default(),
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
        });

    next_finished_screen_state.set(FinishedScreenState::Locked);
}

fn finished_screen_timer(
    mut commands: Commands,
    time: Res<Time>,
    mut finished_text_query: Query<(Entity, &mut FinishedText)>,
    input_mode: Res<InputMode>,
    asset_server: Res<AssetServer>,
    mut next_finished_screen_state: ResMut<NextState<FinishedScreenState>>,
) {
    let (finished_text_entity, mut finished_text) = finished_text_query.single_mut();

    if !finished_text.timer.tick(time.delta()).just_finished() {
        return;
    }

    commands
        .entity(finished_text_entity)
        .with_children(|parent| {
            parent.spawn((
                Name::new("Restart text"),
                TextBundle::from_section(
                    match *input_mode {
                        InputMode::Mouse => "Click to restart",
                        InputMode::Touch => "Tap to restart",
                    },
                    TextStyle {
                        font: asset_server.load(FONT_PATH),
                        font_size: 40.,
                        color: Color::WHITE,
                    },
                ),
            ));
        });

    next_finished_screen_state.set(FinishedScreenState::PromptRestart);
}

fn restart_game(
    mut commands: Commands,
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut touch_events: EventReader<TouchInput>,
    mut next_gamestate: ResMut<NextState<GameState>>,
) {
    if mouse_input.just_pressed(MouseButton::Left)
        || touch_events.read().any(|t| t.phase == TouchPhase::Started)
    {
        commands.remove_resource::<GameResult>();
        next_gamestate.set(GameState::Playing);
        info!("Restarting game");
    }
}
