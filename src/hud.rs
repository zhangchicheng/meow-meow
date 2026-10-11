use bevy::prelude::*;

use crate::{
    AfterRestart, GameState, restart_into,
    actor::Health,
    assets::GameAssets,
    combat::Gun,
    player::{Player, Score},
};

const CREAM: Color = Color::srgb(0.949, 0.827, 0.671);
const PINK: Color = Color::srgb(0.776, 0.624, 0.647);
const DARK: Color = Color::srgba(0.098, 0.098, 0.169, 0.85);
const HUD_LOWER_WIDTH: f32 = 220.0;
/// Deep purple that sits between the tileset's #494d7e and #8b6d9c.
const HP_PURPLE: Color = Color::srgb_u8(0x6e, 0x4f, 0x8a);

pub fn plugin(app: &mut App) {
    app.init_resource::<Score>()
        .init_resource::<Banner>()
        .add_systems(OnEnter(GameState::Playing), spawn_hud)
        .add_systems(OnEnter(GameState::GameOver), |mut banner: ResMut<Banner>| {
            banner.text = "GAME OVER\n\nR  restart     ESC  menu".into();
            banner.timer = None;
        })
        .add_systems(Update, (update_hud, update_banner))
        .add_systems(Update, restart.run_if(in_state(GameState::GameOver)))
        .add_systems(
            Update,
            back_to_menu.run_if(in_state(GameState::Playing).or_else(in_state(GameState::GameOver))),
        );
}

/// Big centred message (wave number, game over).
#[derive(Resource, Default)]
pub struct Banner {
    text: String,
    /// Hide after the timer ends; `None` keeps the text up.
    timer: Option<Timer>,
}

impl Banner {
    pub fn show(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.timer = Some(Timer::from_seconds(2.0, TimerMode::Once));
    }
}

#[derive(Component)]
struct HpBar;
#[derive(Component)]
struct HpText;
#[derive(Component)]
struct ScoreText;
#[derive(Component)]
struct AmmoText;
#[derive(Component)]
struct BannerText;

fn spawn_hud(mut commands: Commands, assets: Res<GameAssets>, mut score: ResMut<Score>, mut banner: ResMut<Banner>) {
    score.0 = 0;
    *banner = Banner::default();
    let font = |size: f32| TextFont { font: assets.font.clone().into(), font_size: size.into(), ..default() };

    commands.spawn((
        Node { width: percent(100), height: percent(100), ..default() },
        DespawnOnEnter(GameState::Restart),
        children![
            // HP keeps its full width; score and ammo share a narrower lower panel.
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: px(16),
                    top: px(14),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexStart,
                    row_gap: px(6),
                    ..default()
                },
                children![
                    (
                        Node {
                            column_gap: px(10),
                            align_items: AlignItems::Center,
                            padding: UiRect::axes(px(12), px(8)),
                            border: UiRect::all(px(2)),
                            border_radius: BorderRadius::px(14.0, 0.0, 14.0, 0.0),
                            ..default()
                        },
                        BackgroundColor(DARK),
                        BorderColor::all(PINK.with_alpha(0.5)),
                        children![
                            (Text::new("HP"), font(22.0), TextColor(CREAM)),
                            (
                                Node {
                                    width: px(160),
                                    height: px(14),
                                    border: UiRect::all(px(2)),
                                    border_radius: BorderRadius::px(5.0, 0.0, 5.0, 0.0),
                                    ..default()
                                },
                                BackgroundColor(DARK),
                                BorderColor::all(CREAM),
                                children![(
                                    HpBar,
                                    Node {
                                        width: percent(100),
                                        height: percent(100),
                                        border_radius: BorderRadius::px(3.0, 0.0, 3.0, 0.0),
                                        ..default()
                                    },
                                    BackgroundColor(HP_PURPLE),
                                )],
                            ),
                            (HpText, Text::new(""), font(22.0), TextColor(CREAM)),
                        ],
                    ),
                    (
                        Node {
                            min_width: px(HUD_LOWER_WIDTH),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(6),
                            padding: UiRect::axes(px(12), px(8)),
                            border: UiRect::all(px(2)),
                            border_radius: BorderRadius::px(14.0, 0.0, 14.0, 0.0),
                            ..default()
                        },
                        BackgroundColor(DARK),
                        BorderColor::all(PINK.with_alpha(0.5)),
                        children![
                            (ScoreText, Text::new(""), font(22.0), TextColor(PINK)),
                            (AmmoText, Text::new(""), font(22.0), TextColor(PINK)),
                        ],
                    ),
                ],
            ),
            // Centre banner.
            (
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    top: percent(28),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                children![(
                    BannerText,
                    Text::new(""),
                    font(48.0),
                    TextColor(CREAM),
                    TextLayout::justify(Justify::Center),
                    TextShadow::default(),
                )],
            ),
            // Controls hint.
            (
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    bottom: px(12),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                children![(
                    Text::new("WASD move   MOUSE aim   LMB shoot   R reload   SPACE / RMB roll   ESC menu"),
                    font(18.0),
                    TextColor(PINK.with_alpha(0.8)),
                )],
            ),
        ],
    ));
}

fn update_hud(
    score: Res<Score>,
    players: Query<(&Health, &Children), With<Player>>,
    guns: Query<&Gun>,
    mut bar: Query<&mut Node, With<HpBar>>,
    mut texts: ParamSet<(
        Query<&mut Text, With<HpText>>,
        Query<&mut Text, With<ScoreText>>,
        Query<(&mut Text, &mut TextColor), With<AmmoText>>,
    )>,
) {
    if let Ok((health, children)) = players.single() {
        if let Some(gun) = children.iter().find_map(|c| guns.get(c).ok())
            && let Ok((mut text, mut color)) = texts.p2().single_mut()
        {
            if gun.is_reloading() {
                text.0 = "RELOADING...".into();
                color.0 = CREAM;
            } else {
                text.0 = format!("AMMO {}/{}", gun.ammo, gun.spec.magazine);
                color.0 = PINK;
            }
        }
        let hp = health.hp.max(0);
        if let Ok(mut node) = bar.single_mut() {
            node.width = percent(100.0 * hp as f32 / health.max as f32);
        }
        if let Ok(mut text) = texts.p0().single_mut() {
            text.0 = format!("{hp}/{}", health.max);
        }
    }
    if let Ok(mut text) = texts.p1().single_mut() {
        text.0 = format!("SCORE {}", score.0);
    }
}

fn update_banner(time: Res<Time>, mut banner: ResMut<Banner>, mut texts: Query<&mut Text, With<BannerText>>) {
    if let Some(timer) = &mut banner.timer
        && timer.tick(time.delta()).just_finished()
    {
        banner.text.clear();
    }
    for mut text in &mut texts {
        if text.0 != banner.text {
            text.0.clone_from(&banner.text);
        }
    }
}

fn restart(keys: Res<ButtonInput<KeyCode>>, mut after: ResMut<AfterRestart>, mut next: ResMut<NextState<GameState>>) {
    if keys.just_pressed(KeyCode::KeyR) {
        restart_into(GameState::Playing, &mut after, &mut next);
    }
}

fn back_to_menu(keys: Res<ButtonInput<KeyCode>>, mut after: ResMut<AfterRestart>, mut next: ResMut<NextState<GameState>>) {
    if keys.just_pressed(KeyCode::Escape) {
        restart_into(GameState::Menu, &mut after, &mut next);
    }
}
