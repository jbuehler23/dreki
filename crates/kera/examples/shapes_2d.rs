//! 2D Shapes — circles, rectangles, triangles, and polygons.

use kera::prelude::*;

fn main() {
    env_logger::init();

    App::new()
        .add_plugins(DefaultPlugins)
        .set_title("kera — 2d shapes")
        .insert_resource(ClearColor([0.1, 0.1, 0.15, 1.0]))
        .add_startup_system(setup)
        .add_system(move_camera)
        .run();
}

fn setup(world: &mut World) {
    world.spawn((Transform::default(), Camera2d));

    // ── Row 1: Basic shapes ────────────────────────────────────────────

    // Circle
    world.spawn((
        Transform::from_xy(-300.0, 150.0),
        Shape2d::circle(50.0).color(Color::RED),
    ));

    // Rectangle
    world.spawn((
        Transform::from_xy(-100.0, 150.0),
        Shape2d::rectangle(120.0, 80.0).color(Color::GREEN),
    ));

    // Triangle
    world.spawn((
        Transform::from_xy(100.0, 150.0),
        Shape2d::triangle(
            Vec2::new(0.0, 50.0),
            Vec2::new(-50.0, -40.0),
            Vec2::new(50.0, -40.0),
        )
        .color(Color::BLUE),
    ));

    // Pentagon (polygon)
    let pentagon = (0..5)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / 5.0 - std::f32::consts::FRAC_PI_2;
            Vec2::new(angle.cos() * 50.0, angle.sin() * 50.0)
        })
        .collect();
    world.spawn((
        Transform::from_xy(300.0, 150.0),
        Shape2d::polygon(pentagon).color(Color::rgb(1.0, 0.6, 0.0)),
    ));

    // ── Row 2: Overlapping with transparency ───────────────────────────

    world.spawn((
        Transform::from_xyz(-200.0, -50.0, 0.0),
        Shape2d::circle(60.0).color(Color::rgba(1.0, 0.0, 0.0, 0.6)),
    ));
    world.spawn((
        Transform::from_xyz(-150.0, -50.0, 0.1),
        Shape2d::circle(60.0).color(Color::rgba(0.0, 1.0, 0.0, 0.6)),
    ));
    world.spawn((
        Transform::from_xyz(-175.0, -10.0, 0.2),
        Shape2d::circle(60.0).color(Color::rgba(0.0, 0.0, 1.0, 0.6)),
    ));

    // ── Row 2: Rotated and scaled shapes ───────────────────────────────

    // Rotated rectangle
    world.spawn((
        Transform {
            translation: glam::Vec3::new(100.0, -50.0, 0.0),
            rotation: Quat::from_rotation_z(0.4),
            ..Default::default()
        },
        Shape2d::rectangle(100.0, 40.0).color(Color::rgb(0.4, 0.8, 1.0)),
    ));

    // Scaled triangle
    world.spawn((
        Transform {
            translation: glam::Vec3::new(300.0, -50.0, 0.0),
            scale: glam::Vec3::new(1.5, 0.8, 1.0),
            ..Default::default()
        },
        Shape2d::triangle(
            Vec2::new(0.0, 40.0),
            Vec2::new(-35.0, -30.0),
            Vec2::new(35.0, -30.0),
        )
        .color(Color::rgb(1.0, 0.3, 0.8)),
    ));

    // ── Row 3: Various polygons ────────────────────────────────────────

    // Hexagon
    let hexagon = (0..6)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / 6.0;
            Vec2::new(angle.cos() * 45.0, angle.sin() * 45.0)
        })
        .collect();
    world.spawn((
        Transform::from_xy(-300.0, -220.0),
        Shape2d::polygon(hexagon).color(Color::rgb(0.8, 0.2, 1.0)),
    ));

    // Star (8-pointed, alternating radii)
    let star: Vec<Vec2> = (0..16)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / 16.0 - std::f32::consts::FRAC_PI_2;
            let r = if i % 2 == 0 { 50.0 } else { 25.0 };
            Vec2::new(angle.cos() * r, angle.sin() * r)
        })
        .collect();
    world.spawn((
        Transform::from_xy(-100.0, -220.0),
        Shape2d::polygon(star).color(Color::rgb(1.0, 1.0, 0.0)),
    ));

    // Arrow shape
    let arrow = vec![
        Vec2::new(0.0, 50.0),     // tip
        Vec2::new(-30.0, 10.0),   // left wing
        Vec2::new(-12.0, 10.0),   // left notch
        Vec2::new(-12.0, -50.0),  // bottom left
        Vec2::new(12.0, -50.0),   // bottom right
        Vec2::new(12.0, 10.0),    // right notch
        Vec2::new(30.0, 10.0),    // right wing
    ];
    world.spawn((
        Transform::from_xy(100.0, -220.0),
        Shape2d::polygon(arrow).color(Color::rgb(0.2, 1.0, 0.5)),
    ));

    // Diamond
    let diamond = vec![
        Vec2::new(0.0, 55.0),
        Vec2::new(35.0, 0.0),
        Vec2::new(0.0, -55.0),
        Vec2::new(-35.0, 0.0),
    ];
    world.spawn((
        Transform::from_xy(300.0, -220.0),
        Shape2d::polygon(diamond).color(Color::rgb(0.0, 0.8, 0.8)),
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
