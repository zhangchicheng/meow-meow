//! Development helpers driven by environment variables:
//!
//! * `NANO_SCREENSHOT=out.png` saves screenshots at the times (seconds) listed in
//!   `NANO_SCREENSHOT_AT=3,8,15` (default 4) and quits shortly after the last one.
//! * `NANO_SPIN=1` moves the aim around the player in a circle (for tuning how the gun is held).
//! * `NANO_AUTOPLAY=1` makes the player shoot the nearest enemy (or barrel) and restart
//!   after a game over, handy for watching the game without input.
//! * F6 toggles 0.1x speed and muzzle markers; F7 pauses; F8 steps 1/60s while paused.
//! * `NANO_MUZZLE_DEBUG=1` starts with slow motion and muzzle markers enabled.
//! * F9 toggles prop/actor collision boxes (off by default); F10 toggles blocking tiles.

use bevy::{prelude::*, time::TimeSystems, render::view::screenshot::{Screenshot, save_to_disk}};
use std::time::Duration;

use crate::{
    AfterRestart, GameState, restart_into,
    actor::{Actor, Dead, FEET},
    map::{Explosive, Level, Obstacle, BULLET_MARGIN, BLOCK_MOVE, BLOCK_SHOT, TILE},
    camera::CursorWorld,
    enemy::Enemy,
    player::Player,
    combat::Gun,
};

pub fn plugin(app: &mut App) {
    let slow = std::env::var("NANO_MUZZLE_DEBUG").is_ok();
    if slow {
        app.world_mut().resource_mut::<Time<Virtual>>().set_relative_speed(0.1);
    }
    app.insert_resource(MuzzleInspection { slow, step: false })
        .insert_resource(CollisionInspection {
            props: std::env::var("NANO_COLLIDERS").is_ok_and(|value| value == "1"),
            tiles: false,
        })
        .add_systems(Update, collision_controls)
        .add_systems(First, inspection_controls.in_set(InspectionControls).before(TimeSystems))
        .add_systems(First, step_inspection.after(TimeSystems))
        .add_systems(PostUpdate, (draw_muzzle_markers, draw_colliders).after(TransformSystems::Propagate));
    if let Ok(path) = std::env::var("NANO_SCREENSHOT") {
        let mut times: Vec<f32> = std::env::var("NANO_SCREENSHOT_AT")
            .ok()
            .map(|s| s.split(',').filter_map(|t| t.trim().parse().ok()).collect())
            .unwrap_or_default();
        if times.is_empty() {
            times.push(4.0);
        }
        app.add_systems(Update, move |time: Res<Time<Real>>, mut commands: Commands, mut taken: Local<usize>, mut exit: MessageWriter<AppExit>| {
            let t = time.elapsed_secs();
            if let Some(&at) = times.get(*taken)
                && t > at
            {
                // `shot.png` -> `shot.png` for a single capture, `shot-1.png`, `shot-2.png`... for several.
                let file = if times.len() == 1 { path.clone() } else { path.replace(".png", &format!("-{}.png", *taken + 1)) };
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(file));
                *taken += 1;
            }
            if times.last().is_some_and(|&last| t > last + 1.0) {
                exit.write(AppExit::Success);
            }
        });
    }
    if std::env::var("NANO_SPIN").is_ok() || std::env::var("NANO_AUTOPLAY").is_ok() {
        app.add_systems(Update, skip_menu.run_if(in_state(GameState::Menu)));
    }
    if std::env::var("NANO_SPIN").is_ok() {
        app.add_systems(Update, spin.after(crate::camera::update_cursor).before(crate::player::control_player));
    }
    if std::env::var("NANO_AUTOPLAY").is_ok() {
        app.add_systems(Update, autoplay.after(crate::camera::update_cursor).before(crate::player::control_player));
    }
}

#[derive(Resource)]
struct CollisionInspection {
    props: bool,
    tiles: bool,
}

fn collision_controls(keys: Res<ButtonInput<KeyCode>>, mut inspection: ResMut<CollisionInspection>) {
    if keys.just_pressed(KeyCode::F9) {
        inspection.props = !inspection.props;
        info!("Prop collision boxes: {}", if inspection.props { "on" } else { "off" });
    }
    if keys.just_pressed(KeyCode::F10) {
        inspection.tiles = !inspection.tiles;
        info!("Blocking tile boxes: {}", if inspection.tiles { "on" } else { "off" });
    }
}

fn draw_colliders(
    inspection: Res<CollisionInspection>,
    obstacles: Query<&Obstacle>,
    portals: Query<&crate::portal::Portal>,
    actors: Query<&GlobalTransform, (With<Actor>, Without<Dead>)>,
    level: Option<Res<Level>>,
    mut gizmos: Gizmos,
) {
    if inspection.props {
        let movement_color = Color::srgb(0.2, 1.0, 0.3);
        let bullet_color = Color::srgb(1.0, 0.65, 0.1);
        let feet_color = Color::srgb(0.1, 0.9, 1.0);
        for obstacle in &obstacles {
            // Draw from the exact world-space data used by collision checks.
            gizmos.rect_2d(obstacle.center, obstacle.half * 2.0, movement_color);
            gizmos.rect_2d(obstacle.center, (obstacle.half + BULLET_MARGIN) * 2.0, bullet_color);
            gizmos.line_2d(obstacle.center - Vec2::X * 1.5, obstacle.center + Vec2::X * 1.5, movement_color);
            gizmos.line_2d(obstacle.center - Vec2::Y * 1.5, obstacle.center + Vec2::Y * 1.5, movement_color);
        }
        for transform in &actors {
            gizmos.rect_2d(transform.translation().truncate(), FEET * 2.0, feet_color);
        }
        for portal in &portals {
            gizmos.rect_2d(portal.center, portal.half * 2.0, Color::srgb(0.8, 0.3, 1.0));
        }
    }
    if inspection.tiles {
        let Some(level) = level else { return };
        for y in 0..level.height {
            for x in 0..level.width {
                let flags = level.cell(x, y);
                if flags & BLOCK_MOVE == 0 { continue; }
                let color = if flags & BLOCK_SHOT != 0 {
                    Color::srgb(1.0, 0.2, 0.3)
                } else {
                    Color::srgb(0.3, 0.5, 1.0)
                };
                gizmos.rect_2d(level.cell_center(IVec2::new(x, y)), Vec2::splat(TILE), color);
            }
        }
    }
}

#[derive(Resource)]
pub(crate) struct MuzzleInspection {
    slow: bool,
    step: bool,
}

impl MuzzleInspection {
    pub fn speed(&self) -> f32 {
        if self.slow { 0.1 } else { 1.0 }
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct InspectionControls;

fn inspection_controls(
    keys: Res<ButtonInput<KeyCode>>,
    mut inspection: ResMut<MuzzleInspection>,
    mut time: ResMut<Time<Virtual>>,
) {
    // First reads the previous frame's input, before the virtual clock advances.
    if keys.just_pressed(KeyCode::F6) {
        inspection.slow = !inspection.slow;
        time.set_relative_speed(if inspection.slow { 0.1 } else { 1.0 });
        info!("Muzzle inspection: {}", if inspection.slow { "0.1x, markers on" } else { "normal speed, markers off" });
    }
    if keys.just_pressed(KeyCode::F7) {
        time.toggle();
        info!("Game time: {}", if time.is_paused() { "paused" } else { "running" });
    }
    if keys.just_pressed(KeyCode::F8) {
        time.pause();
        inspection.step = true;
    }
}

fn step_inspection(
    mut inspection: ResMut<MuzzleInspection>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut time: ResMut<Time>,
) {
    if std::mem::take(&mut inspection.step) {
        // Keep paused, but expose one fixed delta to animation and gameplay systems.
        virtual_time.advance_by(Duration::from_secs_f64(1.0 / 60.0));
        *time = virtual_time.as_generic();
    }
}

fn draw_muzzle_markers(
    inspection: Res<MuzzleInspection>,
    guns: Query<(&Gun, &GlobalTransform, &Visibility)>,
    mut gizmos: Gizmos,
) {
    if !inspection.slow {
        return;
    }
    let muzzle_color = Color::srgb(0.2, 1.0, 0.3);
    let flash_color = Color::srgb(1.0, 0.2, 0.8);
    for (gun, transform, visibility) in &guns {
        if *visibility == Visibility::Hidden {
            continue;
        }
        let muzzle = transform.transform_point(gun.spec.muzzle_position()).truncate();
        let flash = transform.transform_point(gun.spec.flash_position()).truncate();
        gizmos.circle_2d(muzzle, 1.5, muzzle_color).resolution(12);
        gizmos.line_2d(flash - Vec2::X * 2.0, flash + Vec2::X * 2.0, flash_color);
        gizmos.line_2d(flash - Vec2::Y * 2.0, flash + Vec2::Y * 2.0, flash_color);
        gizmos.line_2d(muzzle, flash, flash_color);
    }
}

fn autoplay(
    mut cursor: ResMut<CursorWorld>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    state: Res<State<GameState>>,
    mut after: ResMut<AfterRestart>,
    mut next: ResMut<NextState<GameState>>,
    players: Query<&Transform, (With<Player>, Without<Dead>)>,
    enemies: Query<&Transform, (With<Enemy>, With<Actor>, Without<Dead>)>,
    barrels: Query<&Obstacle, With<Explosive>>,
    obstacles: Query<&Obstacle>,
    level: Option<Res<Level>>,
) {
    let Some(level) = level else { return };
    if *state.get() == GameState::GameOver {
        restart_into(GameState::Playing, &mut after, &mut next);
        return;
    }
    let Ok(player) = players.single() else { return };
    let p = player.translation.truncate();
    let body = p + Vec2::new(0.0, 8.0);
    let obstacles: Vec<&Obstacle> = obstacles.iter().collect();
    let nearest = |points: Vec<Vec2>| points.into_iter().min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)));
    // Shoot enemies, or blow up barrels while waiting for the next wave.
    let visible = enemies
        .iter()
        .map(|t| t.translation.truncate() + Vec2::new(0.0, 8.0))
        .filter(|e| level.line_of_sight(&obstacles, body, *e))
        .collect();
    let target = nearest(visible)
        .or_else(|| nearest(barrels.iter().map(|o| o.center).collect()));
    if let Some(target) = target {
        cursor.0 = Some(target);
        mouse.press(MouseButton::Left);
    } else {
        mouse.release(MouseButton::Left);
    }
}



fn spin(time: Res<Time>, mut cursor: ResMut<CursorWorld>, players: Query<&Transform, With<Player>>) {
    let Ok(player) = players.single() else { return };
    // One turn every 4 seconds, counter-clockwise.
    let angle = time.elapsed_secs() * std::f32::consts::TAU / 4.0;
    cursor.0 = Some(player.translation.truncate() + Vec2::new(0.0, 8.0) + Vec2::from_angle(angle) * 40.0);
}

/// Debug modes start playing right away instead of waiting on the title screen.
fn skip_menu(mut after: ResMut<AfterRestart>, mut next: ResMut<NextState<GameState>>) {
    restart_into(GameState::Playing, &mut after, &mut next);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_step_advances_animation_time_once_and_stays_paused() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(MuzzleInspection { slow: true, step: true })
            .add_systems(First, step_inspection.after(TimeSystems));
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        app.update();
        let step = Duration::from_secs_f64(1.0 / 60.0);
        assert_eq!(app.world().resource::<Time>().delta(), step);
        assert_eq!(app.world().resource::<Time<Virtual>>().delta(), step);
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
        let elapsed = app.world().resource::<Time>().elapsed();
        app.update();
        assert_eq!(app.world().resource::<Time>().delta(), Duration::ZERO);
        assert_eq!(app.world().resource::<Time>().elapsed(), elapsed);
    }
}
