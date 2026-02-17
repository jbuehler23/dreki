//! 3D Shapes — spheres, cuboids, cylinders, and planes with PBR materials.

use necs::prelude::*;

fn main() {
    env_logger::init();

    Game::new("necs — 3d shapes")
        .resource(ClearColor([0.08, 0.08, 0.12, 1.0]))
        .resource(AmbientLight {
            color: [1.0, 1.0, 1.0],
            intensity: 0.08,
        })
        .setup(setup)
        .update(rotate_shapes)
        .update(move_camera)
        .run();
}

fn setup(ctx: &mut Context) {
    // Camera
    ctx.spawn("camera")
        .insert(Transform::from_xyz(0.0, 5.0, 12.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y))
        .insert(Camera3d::default());

    // Lights
    ctx.create().insert(DirectionalLight {
        direction: Vec3::new(-0.4, -1.0, -0.3),
        color: [1.0, 0.98, 0.95],
        intensity: 1.2,
    });
    ctx.create()
        .insert(Transform::from_xyz(4.0, 4.0, 4.0))
        .insert(PointLight { color: [1.0, 0.85, 0.7], intensity: 8.0, radius: 15.0 });
    ctx.create()
        .insert(Transform::from_xyz(-4.0, 3.0, -2.0))
        .insert(PointLight { color: [0.6, 0.8, 1.0], intensity: 6.0, radius: 12.0 });

    // Ground plane
    ctx.create()
        .insert(Transform::from_xyz(0.0, 0.0, 0.0))
        .insert(Shape3d::plane(20.0, 20.0).color([0.25, 0.25, 0.28, 1.0]).roughness(0.9));

    // ── Back row: material showcase ────────────────────────────────────

    ctx.create()
        .insert(Transform::from_xyz(-4.0, 1.01, -3.0))
        .insert(Shape3d::sphere(1.0).color([0.9, 0.15, 0.15, 1.0]).roughness(0.8));

    ctx.create()
        .insert(Transform::from_xyz(-1.5, 1.01, -3.0))
        .insert(Shape3d::sphere(1.0).color([0.15, 0.8, 0.2, 1.0]).roughness(0.4));

    ctx.create()
        .insert(Transform::from_xyz(1.5, 1.01, -3.0))
        .insert(Shape3d::sphere(1.0).color([0.2, 0.3, 0.9, 1.0]).roughness(0.1));

    ctx.create()
        .insert(Transform::from_xyz(4.0, 1.01, -3.0))
        .insert(Shape3d::sphere(1.0).color([1.0, 0.766, 0.336, 1.0]).metallic(1.0).roughness(0.3));

    // ── Front row: different shape types ────────────────────────────────

    // Red spinning cube
    ctx.create()
        .insert(Transform::from_xyz(-3.0, 0.76, 2.0))
        .insert(Shape3d::cuboid(1.5, 1.5, 1.5).color([0.85, 0.2, 0.2, 1.0]).metallic(0.3).roughness(0.4))
        .tag("spinning");

    // Tall cyan cylinder
    ctx.create()
        .insert(Transform::from_xyz(-0.5, 1.26, 2.0))
        .insert(Shape3d::cylinder(0.6, 2.5).color([0.1, 0.8, 0.8, 1.0]).roughness(0.3));

    // Wide flat purple cuboid
    ctx.create()
        .insert(Transform::from_xyz(2.5, 0.41, 2.0))
        .insert(Shape3d::cuboid(2.0, 0.8, 1.2).color([0.6, 0.2, 0.8, 1.0]).roughness(0.5));

    // Small spinning metallic sphere on top of the table
    ctx.create()
        .insert(Transform::from_xyz(2.5, 1.22, 2.0))
        .insert(Shape3d::sphere(0.4).color([0.95, 0.95, 0.95, 1.0]).metallic(1.0).roughness(0.05))
        .tag("spinning");

    // Orange spinning cylinder
    ctx.create()
        .insert(Transform::from_xyz(5.0, 0.76, 2.0))
        .insert(Shape3d::cylinder(0.5, 1.5).color([1.0, 0.5, 0.1, 1.0]).metallic(0.5).roughness(0.35))
        .tag("spinning");
}

fn rotate_shapes(ctx: &mut Context) {
    let dt = ctx.time.delta_secs();

    for entity in ctx.world.tagged("spinning") {
        if let Some(transform) = ctx.world.get_mut::<Transform>(entity) {
            transform.rotation *= Quat::from_rotation_y(0.8 * dt);
        }
    }
}

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
