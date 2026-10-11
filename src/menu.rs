//! Title screen: the live arena drifting in the background, a title and
//! Start / Setting / Exit buttons (mouse or W/S + Enter). Start opens character selection.

use bevy::{
    audio::Volume,
    prelude::*,
    window::{MonitorSelection, WindowMode},
};

use crate::{GameState, assets::GameAssets, restart_into};
use crate::player::PlayerCharacter;
use crate::combat::play_sfx;
use bevy_aseprite_ultra::prelude::*;

const CREAM: Color = Color::srgb_u8(0xf2, 0xd3, 0xab);
const PINK: Color = Color::srgb_u8(0xc6, 0x9f, 0xa5);
const MAUVE: Color = Color::srgb_u8(0x8b, 0x6d, 0x9c);
const SLATE: Color = Color::srgb_u8(0x49, 0x4d, 0x7e);
const NIGHT: Color = Color::srgb_u8(0x2b, 0x2b, 0x4b);
const VOID: Color = Color::srgb_u8(0x19, 0x19, 0x2b);

pub fn plugin(app: &mut App) {
    app.init_resource::<Settings>()
        .init_resource::<MenuPage>()
        .init_resource::<Selected>()
        .add_systems(OnEnter(GameState::Menu), |mut page: ResMut<MenuPage>| *page = MenuPage::Main)
        .add_systems(
            Update,
            (navigate, rebuild_menu, style_buttons).chain().run_if(in_state(GameState::Menu)),
        )
        .add_systems(Update, apply_settings.run_if(resource_changed::<Settings>));
}

/// Player options changed from the Setting page.
#[derive(Resource)]
pub struct Settings {
    pub fullscreen: bool,
    pub screen_shake: bool,
    /// Master volume in percent, cycled in steps of 25.
    pub volume: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { fullscreen: false, screen_shake: true, volume: 75 }
    }
}

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum MenuPage {
    #[default]
    Main,
    Character,
    Setting,
}

/// Index of the highlighted button on the current page.
#[derive(Resource, Default)]
struct Selected(usize);

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    Start,
    ConfirmCharacter,
    Setting,
    Exit,
    ToggleFullscreen,
    ToggleShake,
    CycleVolume,
    Back,
}

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
struct MenuButton {
    action: MenuAction,
    index: usize,
}

fn font(assets: &GameAssets, size: f32) -> TextFont {
    TextFont { font: assets.font.clone().into(), font_size: size.into(), ..default() }
}

/// Rebuilds the menu whenever we enter it, switch page, or change a setting
/// (so the On/Off labels update).
fn rebuild_menu(
    mut commands: Commands,
    assets: Res<GameAssets>,
    page: Res<MenuPage>,
    settings: Res<Settings>,
    character: Res<PlayerCharacter>,
    mut selected: ResMut<Selected>,
    roots: Query<Entity, With<MenuRoot>>,
) {
    if !roots.is_empty() && !page.is_changed() && !settings.is_changed() && !character.is_changed() {
        return;
    }
    if page.is_changed() {
        selected.0 = 0;
    }
    for root in &roots {
        commands.entity(root).despawn();
    }

    let on_off = |on: bool| if on { "On" } else { "Off" };
    let (heading, buttons): (Option<&str>, Vec<(String, MenuAction)>) = match *page {
        MenuPage::Main => (
            None,
            vec![
                ("Start".into(), MenuAction::Start),
                ("Setting".into(), MenuAction::Setting),
                ("Exit".into(), MenuAction::Exit),
            ],
        ),
        MenuPage::Character => (
            Some("Character"),
            vec![],
        ),
        MenuPage::Setting => (
            Some("Setting"),
            vec![
                (format!("Fullscreen  {}", on_off(settings.fullscreen)), MenuAction::ToggleFullscreen),
                (format!("Screen Shake  {}", on_off(settings.screen_shake)), MenuAction::ToggleShake),
                (format!("Volume  {}%", settings.volume), MenuAction::CycleVolume),
                ("Back".into(), MenuAction::Back),
            ],
        ),
    };

    let root = commands
        .spawn((
            MenuRoot,
            DespawnOnExit(GameState::Menu),
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(18),
                ..default()
            },
            // Darken the arena behind the menu.
            BackgroundColor(VOID.with_alpha(0.55)),
        ))
        .id();

    commands.entity(root).with_children(|ui| {
        title(ui, &assets, heading);
        if *page == MenuPage::Character {
            ui.spawn((
                Button,
                MenuButton { action: MenuAction::ConfirmCharacter, index: 0 },
                Node {
                    width: px(360),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(px(10)),
                    row_gap: px(6),
                    border: UiRect::all(px(3)),
                    border_radius: BorderRadius::px(14.0, 0.0, 14.0, 0.0),
                    ..default()
                },
                Outline::new(px(3), Val::ZERO, VOID),
                BackgroundColor(NIGHT),
                BorderColor::all(MAUVE),
                children![
                    (
                        // Trim the sprite canvas's empty space without shrinking the character.
                        Node {
                            width: px(80.0 * crate::camera::ZOOM),
                            height: px(140),
                            overflow: Overflow::clip(),
                            ..default()
                        },
                        children![(
                            ImageNode::default(),
                            AseAnimation {
                                aseprite: assets.characters[character.index()].clone(),
                                animation: Animation::tag(&format!("{}Idle", character.prefix())),
                            },
                            Node {
                                position_type: PositionType::Absolute,
                                top: px(-50),
                                width: px(80.0 * crate::camera::ZOOM),
                                height: px(80.0 * crate::camera::ZOOM),
                                ..default()
                            },
                        )],
                    ),
                    (Text::new(format!("<  {}  >", character.name())), font(&assets, 30.0), TextColor(CREAM)),
                    (Text::new("LEFT / RIGHT choose\nENTER start   ESC back"), font(&assets, 20.0), TextColor(PINK), TextLayout::justify(Justify::Center)),
                ],
            ));
        }
        for (index, (label, action)) in buttons.into_iter().enumerate() {
            ui.spawn((
                Button,
                MenuButton { action, index },
                Node {
                    width: px(if *page == MenuPage::Setting { 300 } else { 220 }),
                    height: px(50),
                    border: UiRect::all(px(3)),
                    border_radius: BorderRadius::px(14.0, 0.0, 14.0, 0.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                // Light inner border + dark outer ring, like the reference buttons.
                Outline::new(px(3), Val::ZERO, VOID),
                BackgroundColor(NIGHT),
                BorderColor::all(MAUVE),
                children![(Text::new(label), font(&assets, 30.0), TextColor(PINK))],
            ));
        }
    });
}

/// Big title with two offset copies behind it, for the chromatic "glitch" look.
fn title(ui: &mut ChildSpawnerCommands, assets: &GameAssets, heading: Option<&str>) {
    let (text, size, gap) = match heading {
        None => ("Meow! Meow!", 112.0, 40),
        Some(h) => (h, 64.0, 16),
    };
    let layer = |dx: f32, dy: f32, color: Color| {
        (
            Text::new(text),
            font(assets, size),
            TextColor(color),
            Node { position_type: PositionType::Absolute, left: px(dx), top: px(dy), ..default() },
        )
    };
    ui.spawn(Node { margin: UiRect::bottom(px(gap)), ..default() }).with_children(|t| {
        t.spawn(layer(7.0, 7.0, SLATE));
        t.spawn(layer(-5.0, -3.0, PINK));
        t.spawn((Text::new(text), font(assets, size), TextColor(CREAM)));
    });
}

fn navigate(
    mut commands: Commands,
    assets: Res<GameAssets>,
    keys: Res<ButtonInput<KeyCode>>,
    mut selected: ResMut<Selected>,
    mut page: ResMut<MenuPage>,
    mut settings: ResMut<Settings>,
    mut character: ResMut<PlayerCharacter>,
    mut next: ResMut<NextState<GameState>>,
    mut after: ResMut<crate::AfterRestart>,
    mut exit: MessageWriter<AppExit>,
    buttons: Query<(&MenuButton, &Interaction)>,
    changed: Query<(&MenuButton, &Interaction), Changed<Interaction>>,
) {
    let count = buttons.iter().count();
    if count == 0 {
        return;
    }
    if *page == MenuPage::Character {
        let choices = [PlayerCharacter::One, PlayerCharacter::Three, PlayerCharacter::Four];
        let index = choices.iter().position(|choice| *choice == *character).unwrap_or(0);
        let direction = i32::from(keys.just_pressed(KeyCode::ArrowRight)) - i32::from(keys.just_pressed(KeyCode::ArrowLeft));
        if direction != 0 {
            *character = choices[(index as i32 + direction).rem_euclid(3) as usize];
            play_sfx(&mut commands, &assets.menu_hover, 0.5, 1.0);
        }
    }
    let previous_selection = selected.0;
    if *page != MenuPage::Character && keys.any_just_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
        selected.0 = (selected.0 + 1) % count;
    }
    if *page != MenuPage::Character && keys.any_just_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
        selected.0 = (selected.0 + count - 1) % count;
    }

    let mut action = None;
    for (button, interaction) in &changed {
        match interaction {
            Interaction::Hovered => selected.0 = button.index,
            Interaction::Pressed => action = Some(button.action),
            Interaction::None => {}
        }
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
        action = buttons.iter().find(|(b, _)| b.index == selected.0).map(|(b, _)| b.action);
    }
    if keys.just_pressed(KeyCode::Escape) && *page != MenuPage::Main {
        action = Some(MenuAction::Back);
    }

    if let Some(action) = action {
        let sound = if matches!(action, MenuAction::Back) { &assets.menu_back } else { &assets.menu_confirm };
        play_sfx(&mut commands, sound, 0.7, 1.0);
    } else if selected.0 != previous_selection {
        play_sfx(&mut commands, &assets.menu_hover, 0.5, 1.0);
    }

    match action {
        Some(MenuAction::Start) => *page = MenuPage::Character,
        Some(MenuAction::ConfirmCharacter) => restart_into(GameState::Playing, &mut after, &mut next),
        Some(MenuAction::Setting) => *page = MenuPage::Setting,
        Some(MenuAction::Exit) => {
            exit.write(AppExit::Success);
        }
        Some(MenuAction::ToggleFullscreen) => settings.fullscreen = !settings.fullscreen,
        Some(MenuAction::ToggleShake) => settings.screen_shake = !settings.screen_shake,
        Some(MenuAction::CycleVolume) => settings.volume = (settings.volume + 25) % 125,
        Some(MenuAction::Back) => *page = MenuPage::Main,
        None => {}
    }
}

fn style_buttons(
    selected: Res<Selected>,
    mut buttons: Query<(&MenuButton, &mut BackgroundColor, &mut BorderColor, &Children)>,
    mut texts: Query<&mut TextColor>,
) {
    for (button, mut bg, mut border, children) in &mut buttons {
        let active = button.index == selected.0;
        bg.0 = if active { SLATE } else { NIGHT };
        *border = BorderColor::all(if active { CREAM } else { MAUVE });
        for child in children {
            if let Ok(mut color) = texts.get_mut(*child) {
                color.0 = if active { CREAM } else { PINK };
            }
        }
    }
}

fn apply_settings(settings: Res<Settings>, mut windows: Query<&mut Window>, mut volume: ResMut<GlobalVolume>) {
    // Applies to sounds started from now on; our sound effects are all short.
    volume.volume = Volume::Linear(settings.volume as f32 / 100.0);
    for mut window in &mut windows {
        let mode = if settings.fullscreen {
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        } else {
            WindowMode::Windowed
        };
        if window.mode != mode {
            window.mode = mode;
        }
    }
}
