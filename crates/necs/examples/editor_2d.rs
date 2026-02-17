//! Editor 2D — demonstrates the in-engine editor with a 2D scene.
//!
//! Press **F12** to toggle the editor overlay. Select entities in the
//! hierarchy panel (left) and inspect/edit their transforms in the
//! inspector panel (right).
//!
//! Run with:
//!     cargo run -p necs --example editor_2d --features editor

use necs::prelude::*;

fn main() {
    env_logger::init();

    Game::new("necs — editor 2d demo")
        .resource(ClearColor([0.12, 0.12, 0.18, 1.0]))
        .setup(setup)
        .update(move_camera)
        .update(rotate_spinner)
        .run();
}

/// Custom marker for the spinning entity.
struct Spinner;

fn setup(ctx: &mut Context) {
    // Camera
    ctx.spawn("camera")
        .insert(Transform::default())
        .insert(Camera2d);

    // A named player entity — easily found in the hierarchy.
    ctx.spawn("player")
        .insert(Transform::from_xy(0.0, 0.0))
        .insert(Sprite::new().color(Color::GREEN).size(50.0, 50.0))
        .tag("gameplay");

    // Several enemy entities — tagged and named.
    for i in 0..3 {
        let x = -200.0 + i as f32 * 200.0;
        ctx.spawn(&format!("enemy_{}", i))
            .insert(Transform::from_xy(x, 150.0))
            .insert(Sprite::new().color(Color::RED).size(40.0, 40.0))
            .tag("enemy")
            .tag("gameplay");
    }

    // A spinner — demonstrates live transform editing in inspector.
    ctx.spawn("spinner")
        .insert(Transform::from_xy(0.0, -120.0))
        .insert(Sprite::new().color(Color::rgba(0.3, 0.6, 1.0, 1.0)).size(60.0, 60.0))
        .insert(Spinner)
        .tag("gameplay");

    // Backdrop — unnamed, but still visible in hierarchy as "Entity N".
    ctx.create()
        .insert(Transform::from_xyz(0.0, 0.0, -1.0))
        .insert(Sprite::new().color(Color::rgba(0.2, 0.2, 0.25, 1.0)).size(600.0, 400.0));

    // Entity with children — hierarchy tree nesting.
    let parent = ctx.spawn("parent_group")
        .insert(Transform::from_xy(250.0, -100.0))
        .insert(Sprite::new().color(Color::rgba(1.0, 1.0, 0.0, 0.5)).size(80.0, 80.0))
        .id();

    ctx.world.spawn_child(parent, (
        Transform::from_xy(0.0, 50.0),
        Sprite::new().color(Color::rgba(1.0, 0.5, 0.0, 1.0)).size(30.0, 30.0),
    ));
    ctx.world.spawn_child(parent, (
        Transform::from_xy(0.0, -50.0),
        Sprite::new().color(Color::rgba(0.0, 1.0, 1.0, 1.0)).size(30.0, 30.0),
    ));

    log::info!("Press F12 to open the editor. Select entities to inspect them.");
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

fn rotate_spinner(ctx: &mut Context) {
    let dt = ctx.time.delta_secs();
    // Use query_filtered to avoid borrowing world inside the closure.
    ctx.world.query_filtered::<(&mut Transform,), Spinner>(|_entity, (tf,)| {
        tf.rotation *= Quat::from_rotation_z(1.5 * dt);
    });
}
