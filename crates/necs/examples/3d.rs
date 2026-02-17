//! Hello 3D — A lit rotating cube on a ground plane.
//!
//! Demonstrates Camera3d, Mesh3d, PBR Material, and lighting.

use necs::prelude::*;

fn main() {
    env_logger::init();

    Game::new("necs — hello 3d")
        .resource(ClearColor([0.1, 0.1, 0.15, 1.0]))
        .resource(AmbientLight {
            color: [1.0, 1.0, 1.0],
            intensity: 0.05,
        })
        .setup(setup)
        .update(rotate_cube)
        .update(move_camera)
        .run();
}

fn setup(ctx: &mut Context) {
    // Camera looking at the origin from above and behind
    ctx.spawn("camera")
        .insert(Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y))
        .insert(Camera3d::default());

    // Red cube (slightly metallic)
    ctx.create()
        .insert(Transform::from_xyz(0.0, 0.75, 0.0))
        .insert(Mesh3d::cube())
        .insert(Material {
            base_color: [0.8, 0.1, 0.1, 1.0],
            metallic: 0.3,
            roughness: 0.4,
            ..Default::default()
        })
        .tag("cube");

    // Ground plane (large, gray, fully rough)
    ctx.create()
        .insert(Transform::from_xyz(0.0, 0.0, 0.0).with_scale(10.0))
        .insert(Mesh3d::plane())
        .insert(Material {
            base_color: [0.4, 0.4, 0.4, 1.0],
            metallic: 0.0,
            roughness: 0.9,
            ..Default::default()
        });

    // Blue plastic sphere
    ctx.create()
        .insert(Transform::from_xyz(2.0, 0.5, -1.0))
        .insert(Mesh3d::sphere())
        .insert(Material {
            base_color: [0.2, 0.4, 0.9, 1.0],
            metallic: 0.0,
            roughness: 0.4,
            ..Default::default()
        });

    // Directional light (sun)
    ctx.create().insert(DirectionalLight {
        direction: Vec3::new(-0.5, -1.0, -0.3),
        color: [1.0, 0.98, 0.95],
        intensity: 1.5,
    });

    // Point light (warm, near the cube)
    ctx.create()
        .insert(Transform::from_xyz(2.0, 2.5, 2.0))
        .insert(PointLight {
            color: [1.0, 0.8, 0.6],
            intensity: 5.0,
            radius: 10.0,
        });
}

/// Rotate the cube each frame.
fn rotate_cube(ctx: &mut Context) {
    let dt = ctx.time.delta_secs();

    for entity in ctx.world.tagged("cube") {
        if let Some(transform) = ctx.world.get_mut::<Transform>(entity) {
            transform.rotation *= Quat::from_rotation_y(0.8 * dt);
        }
    }
}

/// Simple WASD camera movement.
fn move_camera(ctx: &mut Context) {
    let dt = ctx.time.delta_secs();
    let speed = 5.0;

    let mut movement = Vec3::ZERO;
    if ctx.input.pressed(KeyCode::KeyW) { movement.z -= 1.0; }
    if ctx.input.pressed(KeyCode::KeyS) { movement.z += 1.0; }
    if ctx.input.pressed(KeyCode::KeyA) { movement.x -= 1.0; }
    if ctx.input.pressed(KeyCode::KeyD) { movement.x += 1.0; }
    if ctx.input.pressed(KeyCode::Space) { movement.y += 1.0; }
    if ctx.input.pressed(KeyCode::ShiftLeft) { movement.y -= 1.0; }

    if movement == Vec3::ZERO {
        return;
    }
    let movement = movement.normalize() * speed * dt;

    let camera = ctx.world.named("camera");
    let transform = ctx.world.get_mut::<Transform>(camera).unwrap();
    let forward = transform.rotation * Vec3::NEG_Z;
    let right = transform.rotation * Vec3::X;
    let up = Vec3::Y;
    transform.translation += forward * movement.z + right * movement.x + up * movement.y;
}
