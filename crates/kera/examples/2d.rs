//! Hello 2D — colored quads with WASD camera movement.

use kera::prelude::*;

fn main() {
    env_logger::init();

    App::new()
        .add_plugins(DefaultPlugins)
        .set_title("kera — hello 2d")
        .insert_resource(ClearColor([0.15, 0.15, 0.2, 1.0]))
        .add_startup_system(setup)
        .add_system(move_camera)
        .run();
}

fn setup(world: &mut World) {
    // Camera at origin
    world.spawn((Transform::default(), Camera2d));

    // Red square
    world.spawn((
        Transform::from_xy(-150.0, 0.0),
        Sprite {
            color: Color::RED,
            size: Vec2::new(100.0, 100.0),
            ..Default::default()
        },
    ));

    // Green square
    world.spawn((
        Transform::from_xy(0.0, 0.0),
        Sprite {
            color: Color::GREEN,
            size: Vec2::new(100.0, 100.0),
            ..Default::default()
        },
    ));

    // Blue square
    world.spawn((
        Transform::from_xy(150.0, 0.0),
        Sprite {
            color: Color::BLUE,
            size: Vec2::new(100.0, 100.0),
            ..Default::default()
        },
    ));

    // Small white square in front (higher Z)
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 1.0),
        Sprite {
            color: Color::WHITE,
            size: Vec2::new(30.0, 30.0),
            ..Default::default()
        },
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
