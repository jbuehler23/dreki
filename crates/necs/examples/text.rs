//! Hello Text — colored text, multiline strings, multiple font sizes, and tweens.

use std::path::PathBuf;

use necs::prelude::*;

fn main() {
    env_logger::init();

    Game::new("necs — hello text")
        .resource(ClearColor([0.1, 0.1, 0.15, 1.0]))
        .setup(setup)
        .update(move_camera)
        .run();
}

fn setup(ctx: &mut Context) {
    ctx.spawn("camera").insert(Transform::default()).insert(Camera2d);

    let font_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets")
        .join("LiberationSans-Regular.ttf");

    let font = ctx.load_font(&font_path.to_string_lossy(), 16.0);
    let font_large = ctx.load_font(&font_path.to_string_lossy(), 32.0);

    // ── Title ────────────────────────────────────────────────────────────
    ctx.create()
        .insert(Transform::from_xyz(-200.0, 250.0, 1.0))
        .insert(Text::new("necs text rendering", font_large));

    // ── Colored text ─────────────────────────────────────────────────────
    ctx.create().insert(Transform::from_xyz(-200.0, 150.0, 1.0)).insert(Text::new("Red", font).color(Color::RED));
    ctx.create().insert(Transform::from_xyz(-100.0, 150.0, 1.0)).insert(Text::new("Green", font).color(Color::GREEN));
    ctx.create().insert(Transform::from_xyz(0.0, 150.0, 1.0)).insert(Text::new("Blue", font).color(Color::BLUE));

    // ── Multiline ────────────────────────────────────────────────────────
    ctx.create()
        .insert(Transform::from_xyz(-200.0, 80.0, 1.0))
        .insert(Text::new("Line 1\nLine 2\nLine 3", font));

    // ── Large font ───────────────────────────────────────────────────────
    ctx.create()
        .insert(Transform::from_xyz(0.0, 80.0, 1.0))
        .insert(Text::new("32px font", font_large).color(Color::rgb(0.6, 0.8, 1.0)));

    // ── Tweened text ─────────────────────────────────────────────────────
    ctx.create()
        .insert(Transform::from_xyz(-200.0, -20.0, 1.0))
        .insert(Text::new("~ sliding text ~", font).color(Color::rgb(1.0, 1.0, 0.0)))
        .insert(Tween::new(
            TweenTarget::TranslationX { start: -200.0, end: 200.0 },
            EaseFunction::SineInOut,
            3.0,
        ).ping_pong());
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
