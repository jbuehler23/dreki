//! Audio — interactive sound demo.
//!
//! SPACE = play blip sound effect
//! M     = toggle background music pause/resume
//! UP/DOWN = adjust music volume

use std::path::PathBuf;

use dreki::prelude::*;

fn main() {
    env_logger::init();

    Game::new("dreki — audio (SPACE=blip, M=toggle music, UP/DOWN=volume)")
        .resource(ClearColor([0.08, 0.06, 0.12, 1.0]))
        .resource(AudioEngine::new())
        .world_system(audio_system)
        .setup(setup)
        .update(play_blip)
        .update(toggle_music)
        .update(adjust_volume)
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

fn setup(ctx: &mut Context) {
    ctx.spawn("camera").insert(Transform::default()).insert(Camera2d);

    // Load sound data.
    let blip = SoundData::from_file(asset_path("sounds/blip.ogg"))
        .expect("Failed to load blip.ogg");
    ctx.world.insert_resource(BlipSfx(blip));

    // Start background music (looping).
    let music = SoundData::from_file(asset_path("sounds/music.ogg"))
        .expect("Failed to load music.ogg")
        .looping()
        .volume(0.5);
    let handle = ctx.world.resource_mut::<AudioEngine>().play(&music);
    ctx.world.insert_resource(MusicHandle {
        handle,
        paused: false,
        volume: 0.5,
    });

    // Spawn an entity with an AudioSource component.
    let sfx_data = SoundData::from_file(asset_path("sounds/blip.ogg"))
        .expect("Failed to load blip.ogg");
    ctx.create()
        .insert(Transform::default())
        .insert(AudioSource::new(sfx_data).with_volume(0.3));

    // Display instructions via a text entity.
    let font = load_font(&mut ctx.world, &asset_path("LiberationSans-Regular.ttf"), 24.0);
    ctx.create()
        .insert(Transform::from_xyz(-280.0, 200.0, 0.0))
        .insert(Text::new("SPACE = blip  |  M = music  |  UP/DOWN = volume", font)
            .color(Color::rgb(0.7, 0.7, 0.8)));
}

fn play_blip(ctx: &mut Context) {
    if !ctx.input.just_pressed(KeyCode::Space) {
        return;
    }
    let sfx = ctx.world.resource::<BlipSfx>().0.clone();
    ctx.world.resource_mut::<AudioEngine>().play(&sfx);
}

fn toggle_music(ctx: &mut Context) {
    if !ctx.input.just_pressed(KeyCode::KeyM) {
        return;
    }
    let music = ctx.world.resource_mut::<MusicHandle>();
    if music.paused {
        music.handle.resume();
        music.paused = false;
    } else {
        music.handle.pause();
        music.paused = true;
    }
}

fn adjust_volume(ctx: &mut Context) {
    let up = ctx.input.just_pressed(KeyCode::ArrowUp);
    let down = ctx.input.just_pressed(KeyCode::ArrowDown);
    if !up && !down {
        return;
    }

    let music = ctx.world.resource_mut::<MusicHandle>();
    if up {
        music.volume = (music.volume + 0.1).min(1.0);
    }
    if down {
        music.volume = (music.volume - 0.1).max(0.0);
    }
    let vol = music.volume;
    music.handle.set_volume(vol);
}
