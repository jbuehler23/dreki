//! Hello Text — colored text, multiline strings, multiple font sizes, and tweens.

use std::path::PathBuf;

use kera::prelude::*;

fn main() {
    env_logger::init();

    App::new()
        .add_plugins(DefaultPlugins)
        .set_title("kera — hello text")
        .insert_resource(ClearColor([0.1, 0.1, 0.15, 1.0]))
        .add_startup_system(setup)
        .add_system(move_camera)
        .run();
}

fn setup(world: &mut World) {
    world.spawn((Transform::default(), Camera2d));

    let font_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets")
        .join("LiberationSans-Regular.ttf");

    let font = load_font(world, &font_path.to_string_lossy(), 16.0);
    let font_large = load_font(world, &font_path.to_string_lossy(), 32.0);

    // ── Title ────────────────────────────────────────────────────────────
    world.spawn((
        Transform::from_xyz(-200.0, 250.0, 1.0),
        Text::new("kera text rendering", font_large),
    ));

    // ── Colored text ─────────────────────────────────────────────────────
    world.spawn((
        Transform::from_xyz(-200.0, 150.0, 1.0),
        Text::new("Red", font).color(Color::RED),
    ));
    world.spawn((
        Transform::from_xyz(-100.0, 150.0, 1.0),
        Text::new("Green", font).color(Color::GREEN),
    ));
    world.spawn((
        Transform::from_xyz(0.0, 150.0, 1.0),
        Text::new("Blue", font).color(Color::BLUE),
    ));

    // ── Multiline ────────────────────────────────────────────────────────
    world.spawn((
        Transform::from_xyz(-200.0, 80.0, 1.0),
        Text::new("Line 1\nLine 2\nLine 3", font),
    ));

    // ── Large font ───────────────────────────────────────────────────────
    world.spawn((
        Transform::from_xyz(0.0, 80.0, 1.0),
        Text::new("32px font", font_large).color(Color::rgb(0.6, 0.8, 1.0)),
    ));

    // ── Tweened text ─────────────────────────────────────────────────────
    world.spawn((
        Transform::from_xyz(-200.0, -20.0, 1.0),
        Text::new("~ sliding text ~", font).color(Color::rgb(1.0, 1.0, 0.0)),
        Tween::new(
            TweenTarget::TranslationX { start: -200.0, end: 200.0 },
            EaseFunction::SineInOut,
            3.0,
        )
        .ping_pong(),
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
