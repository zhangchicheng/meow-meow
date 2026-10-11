use bevy::{audio::Volume, prelude::*};

use crate::{assets::GameAssets, menu::Settings};

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, start_music)
        .add_systems(Update, update_music_volume);
}

#[derive(Component)]
struct BackgroundMusic;

fn start_music(mut commands: Commands, assets: Res<GameAssets>) {
    // Keep one player alive across menu transitions and game restarts.
    commands.spawn((BackgroundMusic, AudioPlayer(assets.music.clone()), PlaybackSettings::LOOP));
}

fn update_music_volume(settings: Res<Settings>, mut music: Query<&mut AudioSink, With<BackgroundMusic>>) {
    for mut sink in &mut music {
        // GlobalVolume only affects new sounds; update the existing music sink too.
        if settings.is_changed() || sink.is_added() {
            sink.set_volume(Volume::Linear(settings.volume as f32 / 100.0));
        }
    }
}
