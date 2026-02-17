//! 2D Shapes — circles, rectangles, triangles, and polygons.

use necs::prelude::*;

fn main() {
    env_logger::init();

    Game::new("necs — 2d shapes")
        .resource(ClearColor([0.1, 0.1, 0.15, 1.0]))
        .setup(setup)
        .update(move_camera)
        .run();
}

fn setup(ctx: &mut Context) {
    ctx.spawn("camera").insert(Transform::default()).insert(Camera2d);

    // ── Row 1: Basic shapes ────────────────────────────────────────────

    // Circle
    ctx.create()
        .insert(Transform::from_xy(-300.0, 150.0))
        .insert(Shape2d::circle(50.0).color(Color::RED));

    // Rectangle
    ctx.create()
        .insert(Transform::from_xy(-100.0, 150.0))
        .insert(Shape2d::rectangle(120.0, 80.0).color(Color::GREEN));

    // Triangle
    ctx.create()
        .insert(Transform::from_xy(100.0, 150.0))
        .insert(Shape2d::triangle(
            Vec2::new(0.0, 50.0),
            Vec2::new(-50.0, -40.0),
            Vec2::new(50.0, -40.0),
        ).color(Color::BLUE));

    // Pentagon (polygon)
    let pentagon = (0..5)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / 5.0 - std::f32::consts::FRAC_PI_2;
            Vec2::new(angle.cos() * 50.0, angle.sin() * 50.0)
        })
        .collect();
    ctx.create()
        .insert(Transform::from_xy(300.0, 150.0))
        .insert(Shape2d::polygon(pentagon).color(Color::rgb(1.0, 0.6, 0.0)));

    // ── Row 2: Overlapping with transparency ───────────────────────────

    ctx.create()
        .insert(Transform::from_xyz(-200.0, -50.0, 0.0))
        .insert(Shape2d::circle(60.0).color(Color::rgba(1.0, 0.0, 0.0, 0.6)));
    ctx.create()
        .insert(Transform::from_xyz(-150.0, -50.0, 0.1))
        .insert(Shape2d::circle(60.0).color(Color::rgba(0.0, 1.0, 0.0, 0.6)));
    ctx.create()
        .insert(Transform::from_xyz(-175.0, -10.0, 0.2))
        .insert(Shape2d::circle(60.0).color(Color::rgba(0.0, 0.0, 1.0, 0.6)));

    // ── Row 2: Rotated and scaled shapes ───────────────────────────────

    ctx.create()
        .insert(Transform {
            translation: glam::Vec3::new(100.0, -50.0, 0.0),
            rotation: Quat::from_rotation_z(0.4),
            ..Default::default()
        })
        .insert(Shape2d::rectangle(100.0, 40.0).color(Color::rgb(0.4, 0.8, 1.0)));

    ctx.create()
        .insert(Transform {
            translation: glam::Vec3::new(300.0, -50.0, 0.0),
            scale: glam::Vec3::new(1.5, 0.8, 1.0),
            ..Default::default()
        })
        .insert(Shape2d::triangle(
            Vec2::new(0.0, 40.0),
            Vec2::new(-35.0, -30.0),
            Vec2::new(35.0, -30.0),
        ).color(Color::rgb(1.0, 0.3, 0.8)));

    // ── Row 3: Various polygons ────────────────────────────────────────

    let hexagon = (0..6)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / 6.0;
            Vec2::new(angle.cos() * 45.0, angle.sin() * 45.0)
        })
        .collect();
    ctx.create()
        .insert(Transform::from_xy(-300.0, -220.0))
        .insert(Shape2d::polygon(hexagon).color(Color::rgb(0.8, 0.2, 1.0)));

    let star: Vec<Vec2> = (0..16)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / 16.0 - std::f32::consts::FRAC_PI_2;
            let r = if i % 2 == 0 { 50.0 } else { 25.0 };
            Vec2::new(angle.cos() * r, angle.sin() * r)
        })
        .collect();
    ctx.create()
        .insert(Transform::from_xy(-100.0, -220.0))
        .insert(Shape2d::polygon(star).color(Color::rgb(1.0, 1.0, 0.0)));

    let arrow = vec![
        Vec2::new(0.0, 50.0),
        Vec2::new(-30.0, 10.0),
        Vec2::new(-12.0, 10.0),
        Vec2::new(-12.0, -50.0),
        Vec2::new(12.0, -50.0),
        Vec2::new(12.0, 10.0),
        Vec2::new(30.0, 10.0),
    ];
    ctx.create()
        .insert(Transform::from_xy(100.0, -220.0))
        .insert(Shape2d::polygon(arrow).color(Color::rgb(0.2, 1.0, 0.5)));

    let diamond = vec![
        Vec2::new(0.0, 55.0),
        Vec2::new(35.0, 0.0),
        Vec2::new(0.0, -55.0),
        Vec2::new(-35.0, 0.0),
    ];
    ctx.create()
        .insert(Transform::from_xy(300.0, -220.0))
        .insert(Shape2d::polygon(diamond).color(Color::rgb(0.0, 0.8, 0.8)));
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
