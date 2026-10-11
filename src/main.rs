//! Meow! Meow! - a small top-down shooter demo.
//!
//! * Map: Tiled `.tmx` loaded with `bevy_ecs_tiled` (see `assets/maps/arena.tmx`).
//! * Characters, guns, bullets and effects: `.aseprite` files played with `bevy_aseprite_ultra`.

// Bevy systems naturally take many parameters with long query types.
#![allow(clippy::type_complexity, clippy::too_many_arguments)]

use bevy::{prelude::*, window::WindowResolution};
use bevy_aseprite_ultra::prelude::*;
use bevy_ecs_tiled::prelude::*;

mod actor;
mod assets;
mod camera;
mod combat;
mod debug;
mod enemy;
mod hud;
mod map;
mod menu;
mod music;
mod pathfinding;
mod pickups;
mod player;
mod portal;
mod powerups;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    /// Title screen over the live arena.
    #[default]
    Menu,
    Playing,
    GameOver,
    /// Transient state: everything tagged `DespawnOnEnter(Restart)` is cleared,
    /// then we go on to [`AfterRestart`] (a fresh game, or back to the menu).
    Restart,
}

/// State entered once `Restart` has cleared the world.
#[derive(Resource)]
pub struct AfterRestart(pub GameState);

/// Clears the current world (map, actors, HUD...) and then enters `target`.
pub fn restart_into(target: GameState, after: &mut AfterRestart, next: &mut NextState<GameState>) {
    after.0 = target;
    next.set(GameState::Restart);
}

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Meow! Meow!".into(),
                        resolution: WindowResolution::new(1280, 720),
                        canvas: Some("#game".into()),
                        fit_canvas_to_parent: true,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins((TiledPlugin::default(), AsepriteUltraPlugin))
        .insert_resource(ClearColor(Color::srgb_u8(0x19, 0x19, 0x2b)))
        .init_state::<GameState>()
        .insert_resource(AfterRestart(GameState::Playing))
        .add_plugins((
            assets::plugin,
            map::plugin,
            pathfinding::plugin,
            camera::plugin,
            actor::plugin,
            player::plugin,
            portal::plugin,
            powerups::plugin,
            enemy::plugin,
            combat::plugin,
            hud::plugin,
            menu::plugin,
            music::plugin,
            debug::plugin,
        ))
        .add_systems(OnEnter(GameState::Restart), |after: Res<AfterRestart>, mut next: ResMut<NextState<GameState>>| {
            next.set(after.0);
        })
        .run();
}
