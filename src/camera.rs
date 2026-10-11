use bevy::{prelude::*, window::CursorOptions};
use rand::Rng;

use crate::{
    GameState,
    actor::{Actor, BODY_OFFSET},
    map::Level,
    menu::Settings,
    player::Player,
};

/// Screen pixels per art pixel.
pub const ZOOM: f32 = 3.0;

pub fn plugin(app: &mut App) {
    app.init_resource::<CursorWorld>()
        .init_resource::<Shake>()
        .add_systems(Startup, spawn_camera)
        .add_systems(Update, (update_cursor, draw_crosshair.run_if(in_state(GameState::Playing))))
        .add_systems(Update, menu_drift.run_if(in_state(GameState::Menu)))
        // The crosshair replaces the OS cursor while playing.
        .add_systems(OnEnter(GameState::Playing), |mut cursors: Query<&mut CursorOptions>| set_cursor_visible(&mut cursors, false))
        .add_systems(OnExit(GameState::Playing), |mut cursors: Query<&mut CursorOptions>| set_cursor_visible(&mut cursors, true))
        .add_systems(PostUpdate, follow_player.before(TransformSystems::Propagate));
}

/// Mouse position in world space, if the cursor is inside the window.
#[derive(Resource, Default)]
pub struct CursorWorld(pub Option<Vec2>);

/// Camera shake "trauma" in 0..1, decays over time.
#[derive(Resource, Default)]
pub struct Shake(f32);

impl Shake {
    pub fn add(&mut self, amount: f32) {
        self.0 = (self.0 + amount).min(1.0);
    }
}

#[derive(Component)]
pub struct MainCamera {
    /// Smoothed position before shake is applied.
    focus: Vec2,
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        MainCamera { focus: Vec2::ZERO },
        Projection::Orthographic(OrthographicProjection { scale: 1.0 / ZOOM, ..OrthographicProjection::default_2d() }),
    ));
}

pub fn update_cursor(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut cursor: ResMut<CursorWorld>,
) {
    let (Ok(window), Ok((camera, transform))) = (windows.single(), cameras.single()) else { return };
    cursor.0 = window.cursor_position().and_then(|p| camera.viewport_to_world_2d(transform, p).ok());
}

fn set_cursor_visible(cursors: &mut Query<&mut CursorOptions>, visible: bool) {
    for mut cursor in cursors.iter_mut() {
        cursor.visible = visible;
    }
}

/// Slowly pans across the arena behind the title screen.
fn menu_drift(time: Res<Time>, level: Option<Res<Level>>, mut cameras: Query<&mut MainCamera>) {
    let (Some(level), Ok(mut cam)) = (level, cameras.single_mut()) else { return };
    let bounds = level.bounds();
    let t = time.elapsed_secs();
    // A slow Lissajous loop; the camera clamp keeps it inside the map.
    let wander = Vec2::new((t * 0.05).sin() * 0.35, (t * 0.083).sin() * 0.3) * bounds.size();
    cam.focus = bounds.center() + wander;
}

fn draw_crosshair(cursor: Res<CursorWorld>, mut gizmos: Gizmos) {
    let Some(p) = cursor.0 else { return };
    let color = Color::srgb_u8(0xf2, 0xd3, 0xab);
    gizmos.circle_2d(p, 3.5, color).resolution(12);
    for d in [Vec2::X, Vec2::Y, Vec2::NEG_X, Vec2::NEG_Y] {
        gizmos.line_2d(p + d * 5.0, p + d * 7.0, color);
    }
}

fn follow_player(
    time: Res<Time>,
    mut travelled: MessageReader<crate::portal::PortalTravel>,
    level: Option<Res<Level>>,
    cursor: Res<CursorWorld>,
    mut shake: ResMut<Shake>,
    settings: Res<Settings>,
    players: Query<&Transform, (With<Player>, With<Actor>, Without<MainCamera>)>,
    mut cameras: Query<(&mut MainCamera, &mut Transform, &Projection, &Camera)>,
) {
    let teleported = travelled.read().count() > 0;
    let Ok((mut cam, mut transform, projection, camera)) = cameras.single_mut() else { return };
    if let Ok(player) = players.single() {
        let body = player.translation.truncate() + BODY_OFFSET;
        // Look a little towards where the player is aiming.
        let look = cursor.0.map_or(Vec2::ZERO, |c| ((c - body) * 0.2).clamp_length_max(40.0));
        let target = body + look;
        if cam.focus == Vec2::ZERO || teleported {
            cam.focus = target;
        }
        cam.focus = cam.focus.lerp(target, 1.0 - (-8.0 * time.delta_secs()).exp());
    }

    let mut focus = cam.focus;
    if let (Some(level), Some(viewport), Projection::Orthographic(ortho)) =
        (level, camera.logical_viewport_size(), projection)
    {
        let half = viewport * ortho.scale / 2.0;
        let bounds = level.bounds();
        for axis in 0..2 {
            let (lo, hi) = (bounds.min[axis] + half[axis], bounds.max[axis] - half[axis]);
            focus[axis] = if lo < hi { focus[axis].clamp(lo, hi) } else { (bounds.min[axis] + bounds.max[axis]) / 2.0 };
        }
    }

    let mut rng = rand::rng();
    let strength = if settings.screen_shake { shake.0 * shake.0 * 6.0 } else { 0.0 };
    let offset = Vec2::new(rng.random_range(-1.0..=1.0), rng.random_range(-1.0..=1.0)) * strength;
    shake.0 = (shake.0 - time.delta_secs() * 1.5).max(0.0);

    transform.translation = (focus + offset).extend(transform.translation.z);
}
