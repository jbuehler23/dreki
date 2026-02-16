//! Hello 2D — colored quads with WASD camera movement.

use dreki::prelude::*;
fn main() {
    env_logger::init();

    Game::new("dreki — hello 2d")
        .resource(ClearColor([0.15, 0.15, 0.2, 1.0]))
        .setup(setup)
        .update(move_camera)
        .run();
}

fn setup(ctx: &mut Context) {
    // Camera at origin
    ctx.spawn("camera")
        .insert(Transform::default())
        .insert(Camera2d);

    // Red square
    ctx.create()
        .insert(Transform::from_xy(-150.0, 0.0))
        .insert(Sprite::new().color(Color::RED).size(100.0, 100.0));

    // Green square
    ctx.create()
        .insert(Transform::from_xy(0.0, 0.0))
        .insert(Sprite::new().color(Color::GREEN).size(100.0, 100.0));

    // Blue square
    ctx.create()
        .insert(Transform::from_xy(150.0, 0.0))
        .insert(Sprite::new().color(Color::BLUE).size(100.0, 100.0));

    // Small white square in front (higher Z)
    ctx.create()
        .insert(Transform::from_xyz(0.0, 0.0, 1.0))
        .insert(Sprite::new().color(Color::WHITE).size(30.0, 30.0));
}

fn move_camera(ctx: &mut Context) {
    let dt = ctx.time.delta_secs();
    let speed = 200.0;

    let mut dx = 0.0f32;
    let mut dy = 0.0f32;
    if ctx.input.pressed(KeyCode::KeyW) {
        dy += 1.0;
    }
    if ctx.input.pressed(KeyCode::KeyS) {
        dy -= 1.0;
    }
    if ctx.input.pressed(KeyCode::KeyA) {
        dx -= 1.0;
    }
    if ctx.input.pressed(KeyCode::KeyD) {
        dx += 1.0;
    }

    if dx != 0.0 || dy != 0.0 {
        let camera = ctx.world.named("camera");
        let transform = ctx.world.get_mut::<Transform>(camera).unwrap();
        transform.translation.x += dx * speed * dt;
        transform.translation.y += dy * speed * dt;
    }
}
