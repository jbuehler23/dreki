//! 2D Physics — interactive ball pit.
//!
//! Click to spawn balls at the cursor. Press R to reset.
//! WASD to pan the camera. F1 to toggle debug wireframes.

use necs::prelude::*;

fn main() {
    env_logger::init();

    Game::new("necs — 2d physics (click to spawn, F1 wireframes)")
        .resource(ClearColor([0.08, 0.08, 0.12, 1.0]))
        .plugin(Physics2d)
        .resource(PhysicsWorld2d::new().with_gravity(Vec2::new(0.0, -980.0)))
        .resource(DebugColliders2d::default())
        .resource(SpawnCounter(0u32))
        .setup(setup)
        .update(spawn_on_click)
        .update(reset_on_r)
        .update(move_camera)
        .update(toggle_debug_wireframes)
        .run();
}

struct SpawnCounter(u32);

/// Marker for dynamic bodies so we can despawn them on reset.
struct DynamicBall;

fn setup(ctx: &mut Context) {
    ctx.spawn("camera").insert(Transform::default()).insert(Camera2d);
    spawn_arena(&mut ctx.world);
}

fn spawn_arena(world: &mut World) {
    let wall_color = Color::rgb(0.25, 0.25, 0.3);

    // Floor
    world.spawn((
        Transform::from_xy(0.0, -250.0),
        RigidBody2d::fixed(),
        Collider2d::cuboid(350.0, 20.0),
        Shape2d::rectangle(700.0, 40.0).color(wall_color),
    ));

    // Left wall
    world.spawn((
        Transform::from_xy(-330.0, 0.0),
        RigidBody2d::fixed(),
        Collider2d::cuboid(20.0, 300.0),
        Shape2d::rectangle(40.0, 600.0).color(wall_color),
    ));

    // Right wall
    world.spawn((
        Transform::from_xy(330.0, 0.0),
        RigidBody2d::fixed(),
        Collider2d::cuboid(20.0, 300.0),
        Shape2d::rectangle(40.0, 600.0).color(wall_color),
    ));

    // Angled shelf (left) — a narrow fixed platform
    world.spawn((
        Transform {
            translation: glam::Vec3::new(-120.0, -50.0, 0.0),
            rotation: Quat::from_rotation_z(0.3),
            ..Default::default()
        },
        RigidBody2d::fixed(),
        Collider2d::cuboid(100.0, 8.0),
        Shape2d::rectangle(200.0, 16.0).color(wall_color),
    ));

    // Angled shelf (right)
    world.spawn((
        Transform {
            translation: glam::Vec3::new(120.0, -120.0, 0.0),
            rotation: Quat::from_rotation_z(-0.25),
            ..Default::default()
        },
        RigidBody2d::fixed(),
        Collider2d::cuboid(100.0, 8.0),
        Shape2d::rectangle(200.0, 16.0).color(wall_color),
    ));

    // Seed a few initial balls
    let colors = [Color::RED, Color::GREEN, Color::BLUE];
    for (i, &color) in colors.iter().enumerate() {
        let x = (i as f32 - 1.0) * 60.0;
        world.spawn((
            Transform::from_xy(x, 200.0),
            RigidBody2d::dynamic(),
            Collider2d::ball(14.0).with_restitution(0.6),
            Shape2d::circle(14.0).color(color),
            DynamicBall,
        ));
    }
}

fn spawn_on_click(ctx: &mut Context) {
    if !ctx.input.mouse_just_pressed(MouseButton::Left) {
        return;
    }

    // Get cursor position in screen space, window size, and camera transform.
    let cursor = ctx.cursor;
    let gpu = ctx.world.resource::<GpuContext>();
    let win_w = gpu.surface_config.width as f32;
    let win_h = gpu.surface_config.height as f32;

    let mut cam_translation = glam::Vec3::ZERO;
    ctx.world.query_single::<(&Transform,), Camera2d>(|_e, (tf,)| {
        cam_translation = tf.translation;
    });

    // The orthographic projection maps screen center → camera position,
    // with 1 world unit = 1 pixel. Y is flipped (screen Y-down, world Y-up).
    let wx = cursor.x - win_w / 2.0 + cam_translation.x;
    let wy = -(cursor.y - win_h / 2.0) + cam_translation.y;

    // Pick a color based on spawn counter.
    let counter = &mut ctx.world.resource_mut::<SpawnCounter>().0;
    let idx = *counter;
    *counter = counter.wrapping_add(1);
    let color = match idx % 6 {
        0 => Color::RED,
        1 => Color::GREEN,
        2 => Color::BLUE,
        3 => Color::rgb(1.0, 1.0, 0.0),
        4 => Color::rgb(1.0, 0.5, 0.0),
        _ => Color::rgb(0.0, 1.0, 1.0),
    };

    // Vary the radius a little.
    let radius = 10.0 + (idx % 5) as f32 * 4.0;

    ctx.world.spawn((
        Transform::from_xy(wx, wy),
        RigidBody2d::dynamic(),
        Collider2d::ball(radius).with_restitution(0.5 + (idx % 4) as f32 * 0.1),
        Shape2d::circle(radius).color(color),
        DynamicBall,
    ));
}

fn reset_on_r(ctx: &mut Context) {
    if !ctx.input.just_pressed(KeyCode::KeyR) {
        return;
    }

    // Collect dynamic ball entities.
    let mut to_despawn = Vec::new();
    ctx.world.query::<(&DynamicBall,)>(|entity, _| {
        to_despawn.push(entity);
    });
    for e in to_despawn {
        ctx.world.despawn(e);
    }

    // Reset counter.
    ctx.world.resource_mut::<SpawnCounter>().0 = 0;

    // Re-seed initial balls.
    let colors = [Color::RED, Color::GREEN, Color::BLUE];
    for (i, &color) in colors.iter().enumerate() {
        let x = (i as f32 - 1.0) * 60.0;
        ctx.world.spawn((
            Transform::from_xy(x, 200.0),
            RigidBody2d::dynamic(),
            Collider2d::ball(14.0).with_restitution(0.6),
            Shape2d::circle(14.0).color(color),
            DynamicBall,
        ));
    }
}

fn toggle_debug_wireframes(ctx: &mut Context) {
    if ctx.input.just_pressed(KeyCode::F1) {
        let dbg = ctx.world.resource_mut::<DebugColliders2d>();
        dbg.enabled = !dbg.enabled;
    }
}

fn move_camera(ctx: &mut Context) {
    let dt = ctx.time.delta_secs();
    let speed = 300.0;

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
