use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;

pub fn plugin(app: &mut App) {
    app.init_resource::<GameAssets>();
}

/// Handles to every asset the game spawns at runtime.
#[derive(Resource)]
pub struct GameAssets {
    /// Player options: Luna (character1), Iris (character3), Nora (character4).
    pub characters: [Handle<Aseprite>; 3],
    /// Character 2 costume variants: red, yellow, green, blue.
    pub character2_variants: [Handle<Aseprite>; 4],
    pub enemy_guns: [Handle<Aseprite>; 4],
    pub enemy_bullets: [Handle<Aseprite>; 4],
    pub guns: Handle<Aseprite>,
    pub bullets: Handle<Aseprite>,
    pub muzzle_flash: Handle<Aseprite>,
    pub bullet_hit: Handle<Aseprite>,
    /// Nano effects pack 1 (spawn, explosion, ...).
    pub effects1: Handle<Aseprite>,
    /// Nano effects pack 2 (level up, heal, ...).
    pub effects2: Handle<Aseprite>,
    pub font: Handle<Font>,
    pub shot: Handle<AudioSource>,
    pub reload: Handle<AudioSource>,
    pub collect: Handle<AudioSource>,
    pub menu_hover: Handle<AudioSource>,
    pub menu_confirm: Handle<AudioSource>,
    pub menu_back: Handle<AudioSource>,
    pub meow: Handle<AudioSource>,
    pub music: Handle<AudioSource>,
}

impl FromWorld for GameAssets {
    fn from_world(world: &mut World) -> Self {
        let server = world.resource::<AssetServer>();
        Self {
            characters: [1, 3, 4].map(|i| server.load(format!("characters/character{i}.aseprite"))),
            character2_variants: ["red", "yellow", "green", "blue"]
                .map(|color| server.load(format!("characters/character2_{color}.aseprite"))),
            enemy_guns: ["red", "yellow", "green", "blue"]
                .map(|color| server.load(format!("weapons/guns_{color}.aseprite"))),
            enemy_bullets: ["red", "yellow", "green", "blue"]
                .map(|color| server.load(format!("weapons/bullets_{color}.aseprite"))),
            guns: server.load("weapons/guns.aseprite"),
            bullets: server.load("weapons/bullets.aseprite"),
            muzzle_flash: server.load("fx/muzzle_flash.aseprite"),
            bullet_hit: server.load("fx/bullet_hit.aseprite"),
            effects1: server.load("fx/effects1.aseprite"),
            effects2: server.load("fx/effects2.aseprite"),
            font: server.load("fonts/Nanofon.ttf"),
            shot: server.load("audio/gun.wav"),
            reload: server.load("audio/reload.wav"),
            collect: server.load("audio/collect.wav"),
            menu_hover: server.load("audio/menu_hover.wav"),
            menu_confirm: server.load("audio/menu_confirm.wav"),
            menu_back: server.load("audio/menu_back.wav"),
            meow: server.load("audio/meow.wav"),
            music: server.load("music/journey.wav"),
        }
    }
}
