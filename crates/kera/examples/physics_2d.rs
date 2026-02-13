//! 2D Physics — interactive ball pit.
//!
//! Click to spawn balls at the cursor. Press R to reset.
//! WASD to pan the camera.

use kera::prelude::*;

fn main() {
    env_logger::init();

    App::new()
        .add_plugins(DefaultPlugins)
        .set_title("kera — 2d physics (click to spawn, F1 wireframes)")
        .insert_resource(ClearColor([0.08, 0.08, 0.12, 1.0]))
        .insert_resource(PhysicsWorld2d::new().with_gravity(Vec2::new(0.0, -980.0)))
        .insert_resource(SpawnCounter(0u32))
        .insert_resource(DebugColliders2d::default())
        .add_startup_system(setup)
        .add_system(physics_step_2d)
        .add_system(spawn_on_click)
        .add_system(reset_on_r)
        .add_system(move_camera)
        .add_system(toggle_debug_wireframes)
        .run();
}

struct SpawnCounter(u32);

/// Marker for dynamic bodies so we can despawn them on reset.
struct DynamicBall;

fn setup(world: &mut World) {
    world.spawn((Transform::default(), Camera2d));
    spawn_arena(world);
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

fn spawn_on_click(world: &mut World) {
    let clicked = world.resource::<Input<MouseButton>>().just_pressed(MouseButton::Left);
    if !clicked {
        return;
    }

    // Get cursor position in screen space, window size, and camera transform.
    let cursor = *world.resource::<CursorPosition>();
    let gpu = world.resource::<GpuContext>();
    let win_w = gpu.surface_config.width as f32;
    let win_h = gpu.surface_config.height as f32;

    let mut cam_translation = glam::Vec3::ZERO;
    world.query_single::<(&Transform,), Camera2d>(|_e, (tf,)| {
        cam_translation = tf.translation;
    });

    // The orthographic projection maps screen center → camera position,
    // with 1 world unit = 1 pixel. Y is flipped (screen Y-down, world Y-up).
    let wx = cursor.x - win_w / 2.0 + cam_translation.x;
    let wy = -(cursor.y - win_h / 2.0) + cam_translation.y;

    // Pick a color based on spawn counter.
    let counter = &mut world.resource_mut::<SpawnCounter>().0;
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

    world.spawn((
        Transform::from_xy(wx, wy),
        RigidBody2d::dynamic(),
        Collider2d::ball(radius).with_restitution(0.5 + (idx % 4) as f32 * 0.1),
        Shape2d::circle(radius).color(color),
        DynamicBall,
    ));
}

fn reset_on_r(world: &mut World) {
    if !world.resource::<Input<KeyCode>>().just_pressed(KeyCode::KeyR) {
        return;
    }

    // Collect dynamic ball entities.
    let mut to_despawn = Vec::new();
    world.query::<(&DynamicBall,)>(|entity, _| {
        to_despawn.push(entity);
    });
    for e in to_despawn {
        world.despawn(e);
    }

    // Reset counter.
    world.resource_mut::<SpawnCounter>().0 = 0;

    // Re-seed initial balls.
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

fn toggle_debug_wireframes(world: &mut World) {
    if world.resource::<Input<KeyCode>>().just_pressed(KeyCode::F1) {
        let dbg = world.resource_mut::<DebugColliders2d>();
        dbg.enabled = !dbg.enabled;
    }
}

fn move_camera(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let speed = 300.0;

    let input = world.resource::<Input<KeyCode>>();
    let mut dx = 0.0f32;
    let mut dy = 0.0f32;
    if input.pressed(KeyCode::KeyW) { dy += 1.0; }
    if input.pressed(KeyCode::KeyS) { dy -= 1.0; }
    if input.pressed(KeyCode::KeyA) { dx -= 1.0; }
    if input.pressed(KeyCode::KeyD) { dx += 1.0; }

    if dx != 0.0 || dy != 0.0 {
        world.query_single::<(&mut Transform,), Camera2d>(|_e, (tf,)| {
            tf.translation.x += dx * speed * dt;
            tf.translation.y += dy * speed * dt;
        });
    }
}
