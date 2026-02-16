//! Hello Animation — sprite-sheet animation and property tweening.

use std::path::PathBuf;

use dreki::prelude::*;

fn main() {
    env_logger::init();

    Game::new("dreki — hello animation")
        .resource(ClearColor([0.0, 0.0, 0.0, 1.0]))
        .setup(setup)
        .update(move_camera)
        .run();
}

fn setup(ctx: &mut Context) {
    ctx.spawn("camera").insert(Transform::default()).insert(Camera2d);

    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets");

    // ── Sprite sheet animation ──────────────────────────────────────────
    let sheet_path = assets.join("monochrome_tilemap.png");

    if sheet_path.exists() {
        let tex = load_texture(&mut ctx.world, &sheet_path.to_string_lossy());

        let player = SpriteSheet::from_grid(
            Vec2::new(16.0, 16.0), 20, 20, Some(Vec2::new(1.0, 1.0)), None,
        ).play_range(240, 246, 0.5).looping();

        ctx.create()
            .insert(Transform::from_xy(0.0, 150.0))
            .insert(Sprite::new().texture(tex).size(128.0, 128.0))
            .insert(player);
    } else {
        log::warn!("No spritesheet found — skipping sprite sheet demo");
        log::warn!("  expected at: {}", sheet_path.display());
    }

    // ── Tweening demos ──────────────────────────────────────────────────

    // 1) Translation ping-pong (QuadInOut)
    ctx.create()
        .insert(Transform::from_xy(-300.0, -50.0))
        .insert(Sprite::new().color(Color::RED).size(60.0, 60.0))
        .insert(Tween::new(
            TweenTarget::TranslationX { start: -300.0, end: 300.0 },
            EaseFunction::QuadInOut,
            2.0,
        ).ping_pong());

    // 2) Scale pulse (SineInOut)
    ctx.create()
        .insert(Transform::from_xy(-150.0, -50.0))
        .insert(Sprite::new().color(Color::GREEN).size(60.0, 60.0))
        .insert(Tween::new(
            TweenTarget::ScaleUniform { start: 0.5, end: 1.5 },
            EaseFunction::SineInOut,
            1.0,
        ).ping_pong());

    // 3) Rotation loop (Linear)
    ctx.create()
        .insert(Transform::from_xy(0.0, -50.0))
        .insert(Sprite::new().color(Color::BLUE).size(60.0, 60.0))
        .insert(Tween::new(
            TweenTarget::Rotation { start: 0.0, end: std::f32::consts::TAU },
            EaseFunction::Linear,
            2.0,
        ).looping());

    // 4) Color fade (CubicInOut)
    ctx.create()
        .insert(Transform::from_xy(150.0, -50.0))
        .insert(Sprite::new().color(Color::rgba(1.0, 1.0, 1.0, 1.0)).size(60.0, 60.0))
        .insert(Tween::new(
            TweenTarget::ColorA { start: 1.0, end: 0.1 },
            EaseFunction::CubicInOut,
            1.5,
        ).ping_pong());

    // 5) Vertical bounce (CubicOut)
    ctx.create()
        .insert(Transform::from_xy(300.0, -150.0))
        .insert(Sprite::new().color(Color::rgb(1.0, 0.6, 0.0)).size(40.0, 40.0))
        .insert(Tween::new(
            TweenTarget::TranslationY { start: -150.0, end: 50.0 },
            EaseFunction::CubicOut,
            1.0,
        ).ping_pong());

    // ── Text labels ───────────────────────────────────────────────────
    let font_path = assets.join("LiberationSans-Regular.ttf");
    let font = load_font(&mut ctx.world, &font_path.to_string_lossy(), 16.0);

    ctx.create().insert(Transform::from_xyz(-345.0, -120.0, 1.0)).insert(Text::new("X ping-pong", font));
    ctx.create().insert(Transform::from_xyz(-195.0, -120.0, 1.0)).insert(Text::new("Scale pulse", font));
    ctx.create().insert(Transform::from_xyz(-35.0, -120.0, 1.0)).insert(Text::new("Rotation", font));
    ctx.create().insert(Transform::from_xyz(110.0, -120.0, 1.0)).insert(Text::new("Alpha fade", font));
    ctx.create().insert(Transform::from_xyz(265.0, -220.0, 1.0)).insert(Text::new("Y bounce", font));
}

fn move_camera(ctx: &mut Context) {
    let dt = ctx.time.delta_secs();
    let speed = 200.0;

    let mut dx = 0.0f32;
    let mut dy = 0.0f32;
    if ctx.input.pressed(KeyCode::KeyW) { dy += 1.0; }
    if ctx.input.pressed(KeyCode::KeyS) { dy -= 1.0; }
    if ctx.input.pressed(KeyCode::KeyA) { dx -= 1.0; }
    if ctx.input.pressed(KeyCode::KeyD) { dx += 1.0; }

    if dx != 0.0 || dy != 0.0 {
        let camera = ctx.world.named("camera");
        let tf = ctx.world.get_mut::<Transform>(camera).unwrap();
        tf.translation.x += dx * speed * dt;
        tf.translation.y += dy * speed * dt;
    }
}
