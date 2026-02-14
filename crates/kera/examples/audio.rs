//! Audio — interactive sound demo.
//!
//! SPACE = play blip sound effect
//! M     = toggle background music pause/resume
//! UP/DOWN = adjust music volume

use std::path::PathBuf;

use kera::prelude::*;

fn main() {
    env_logger::init();

    App::new()
        .add_plugins(DefaultPlugins)
        .set_title("kera — audio (SPACE=blip, M=toggle music, UP/DOWN=volume)")
        .insert_resource(ClearColor([0.08, 0.06, 0.12, 1.0]))
        .add_plugins(AudioPlugin)
        .add_startup_system(setup)
        .add_system(play_blip)
        .add_system(toggle_music)
        .add_system(adjust_volume)
        .run();
}

/// Stores the blip sound data for one-shot playback.
struct BlipSfx(SoundData);

/// Stores the music handle so we can pause/resume it.
struct MusicHandle {
    handle: SoundHandle,
    paused: bool,
    volume: f64,
}

fn asset_path(relative: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets")
        .join(relative)
        .to_string_lossy()
        .into_owned()
}

fn setup(world: &mut World) {
    // Camera (required for the render2d pass).
    world.spawn((Transform::default(), Camera2d));

    // Load sound data.
    let blip = SoundData::from_file(asset_path("sounds/blip.ogg"))
        .expect("Failed to load blip.ogg");
    world.insert_resource(BlipSfx(blip));

    // Start background music (looping).
    let music = SoundData::from_file(asset_path("sounds/music.ogg"))
        .expect("Failed to load music.ogg")
        .looping()
        .volume(0.5);
    let handle = world.resource_mut::<AudioEngine>().play(&music);
    world.insert_resource(MusicHandle {
        handle,
        paused: false,
        volume: 0.5,
    });

    // Spawn an entity with an AudioSource component (demonstrates component-based audio).
    // This won't auto-play since auto_play is false by default.
    let sfx_data = SoundData::from_file(asset_path("sounds/blip.ogg"))
        .expect("Failed to load blip.ogg");
    world.spawn((
        Transform::default(),
        AudioSource::new(sfx_data).with_volume(0.3),
    ));

    // Display instructions via a text entity.
    let font = load_font(world, &asset_path("LiberationSans-Regular.ttf"), 24.0);
    world.spawn((
        Transform::from_xyz(-280.0, 200.0, 0.0),
        Text::new("SPACE = blip  |  M = music  |  UP/DOWN = volume", font)
            .color(Color::rgb(0.7, 0.7, 0.8)),
    ));
}

fn play_blip(world: &mut World) {
    if !world
        .resource::<Input<KeyCode>>()
        .just_pressed(KeyCode::Space)
    {
        return;
    }
    let sfx = world.resource::<BlipSfx>().0.clone();
    world.resource_mut::<AudioEngine>().play(&sfx);
}

fn toggle_music(world: &mut World) {
    if !world
        .resource::<Input<KeyCode>>()
        .just_pressed(KeyCode::KeyM)
    {
        return;
    }
    let music = world.resource_mut::<MusicHandle>();
    if music.paused {
        music.handle.resume();
        music.paused = false;
    } else {
        music.handle.pause();
        music.paused = true;
    }
}

fn adjust_volume(world: &mut World) {
    let input = world.resource::<Input<KeyCode>>();
    let up = input.just_pressed(KeyCode::ArrowUp);
    let down = input.just_pressed(KeyCode::ArrowDown);
    if !up && !down {
        return;
    }

    let music = world.resource_mut::<MusicHandle>();
    if up {
        music.volume = (music.volume + 0.1).min(1.0);
    }
    if down {
        music.volume = (music.volume - 0.1).max(0.0);
    }
    let vol = music.volume;
    music.handle.set_volume(vol);
}
