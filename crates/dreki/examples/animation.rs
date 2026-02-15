//! Hello Animation — sprite-sheet animation and property tweening.

use std::path::PathBuf;

use dreki::prelude::*;

fn main() {
    env_logger::init();

    App::new()
        .add_plugins(DefaultPlugins)
        .set_title("dreki — hello animation")
        .insert_resource(ClearColor([0.0, 0.0, 0.0, 1.0]))
        .add_startup_system(setup)
        .add_system(move_camera)
        .run();
}

fn setup(world: &mut World) {
    world.spawn((Transform::default(), Camera2d));

    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets");

    // ── Sprite sheet animation ──────────────────────────────────────────
    let sheet_path = assets.join("monochrome_tilemap.png");

    if sheet_path.exists() {
        let tex = load_texture(world, &sheet_path.to_string_lossy());

        let player = SpriteSheet::from_grid(
            Vec2::new(16.0, 16.0), 20, 20, Some(Vec2::new(1.0, 1.0)), None,
        ).play_range(240, 246, 0.5).looping();

        world.spawn((
            Transform::from_xy(0.0, 150.0),
            Sprite {
                texture: Some(tex),
                size: Vec2::new(128.0, 128.0),
                ..Default::default()
            },
            player,
        ));
    } else {
        log::warn!("No spritesheet found — skipping sprite sheet demo");
        log::warn!("  expected at: {}", sheet_path.display());
    }

    // ── Tweening demos ──────────────────────────────────────────────────

    // 1) Translation ping-pong (QuadInOut)
    world.spawn((
        Transform::from_xy(-300.0, -50.0),
        Sprite {
            color: Color::RED,
            size: Vec2::new(60.0, 60.0),
            ..Default::default()
        },
        Tween::new(
            TweenTarget::TranslationX { start: -300.0, end: 300.0 },
            EaseFunction::QuadInOut,
            2.0,
        ).ping_pong(),
    ));

    // 2) Scale pulse (SineInOut)
    world.spawn((
        Transform::from_xy(-150.0, -50.0),
        Sprite {
            color: Color::GREEN,
            size: Vec2::new(60.0, 60.0),
            ..Default::default()
        },
        Tween::new(
            TweenTarget::ScaleUniform { start: 0.5, end: 1.5 },
            EaseFunction::SineInOut,
            1.0,
        ).ping_pong(),
    ));

    // 3) Rotation loop (Linear)
    world.spawn((
        Transform::from_xy(0.0, -50.0),
        Sprite {
            color: Color::BLUE,
            size: Vec2::new(60.0, 60.0),
            ..Default::default()
        },
        Tween::new(
            TweenTarget::Rotation {
                start: 0.0,
                end: std::f32::consts::TAU,
            },
            EaseFunction::Linear,
            2.0,
        ).looping(),
    ));

    // 4) Color fade (CubicInOut)
    world.spawn((
        Transform::from_xy(150.0, -50.0),
        Sprite {
            color: Color::rgba(1.0, 1.0, 1.0, 1.0),
            size: Vec2::new(60.0, 60.0),
            ..Default::default()
        },
        Tween::new(
            TweenTarget::ColorA { start: 1.0, end: 0.1 },
            EaseFunction::CubicInOut,
            1.5,
        ).ping_pong(),
    ));

    // 5) Vertical bounce (CubicOut — fast start, slow end)
    world.spawn((
        Transform::from_xy(300.0, -150.0),
        Sprite {
            color: Color::rgb(1.0, 0.6, 0.0),
            size: Vec2::new(40.0, 40.0),
            ..Default::default()
        },
        Tween::new(
            TweenTarget::TranslationY { start: -150.0, end: 50.0 },
            EaseFunction::CubicOut,
            1.0,
        ).ping_pong(),
    ));

    // ── Text labels ───────────────────────────────────────────────────
    let font_path = assets.join("LiberationSans-Regular.ttf");
    let font = load_font(world, &font_path.to_string_lossy(), 16.0);

    // Offsets account for text being left-aligned (not centered like sprites)
    world.spawn((
        Transform::from_xyz(-345.0, -120.0, 1.0),
        Text::new("X ping-pong", font),
    ));
    world.spawn((
        Transform::from_xyz(-195.0, -120.0, 1.0),
        Text::new("Scale pulse", font),
    ));
    world.spawn((
        Transform::from_xyz(-35.0, -120.0, 1.0),
        Text::new("Rotation", font),
    ));
    world.spawn((
        Transform::from_xyz(110.0, -120.0, 1.0),
        Text::new("Alpha fade", font),
    ));
    world.spawn((
        Transform::from_xyz(265.0, -220.0, 1.0),
        Text::new("Y bounce", font),
    ));
}

fn move_camera(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let speed = 200.0;

    let input = world.resource::<Input<KeyCode>>();
    let mut dx = 0.0f32;
    let mut dy = 0.0f32;
    if input.pressed(KeyCode::KeyW) { dy += 1.0; }
    if input.pressed(KeyCode::KeyS) { dy -= 1.0; }
    if input.pressed(KeyCode::KeyA) { dx -= 1.0; }
    if input.pressed(KeyCode::KeyD) { dx += 1.0; }

    if dx != 0.0 || dy != 0.0 {
        world.query_single::<(&mut Transform,), Camera2d>(|_e, (transform,)| {
            transform.translation.x += dx * speed * dt;
            transform.translation.y += dy * speed * dt;
        });
    }
}
