//! 3D Physics — interactive cube pyramid.
//!
//! Press Space to launch a sphere at the pyramid. Press R to reset.
//! WASD + Space/Shift to move the camera. Mouse click also launches a sphere.

use dreki::prelude::*;

fn main() {
    env_logger::init();

    Game::new("dreki — 3d physics (space to shoot, F1 wireframes)")
        .resource(ClearColor([0.08, 0.08, 0.12, 1.0]))
        .resource(AmbientLight {
            color: [1.0, 1.0, 1.0],
            intensity: 0.1,
        })
        .resource(PhysicsWorld3d::new())
        .resource(DebugColliders3d::default())
        .world_system(physics_step_3d)
        .setup(setup)
        .update(shoot_sphere)
        .update(reset_scene)
        .update(move_camera)
        .update(toggle_debug_wireframes)
        .run();
}

fn setup(ctx: &mut Context) {
    // Camera
    ctx.spawn("camera")
        .insert(Transform::from_xyz(0.0, 6.0, 14.0).looking_at(Vec3::new(0.0, 2.0, 0.0), Vec3::Y))
        .insert(Camera3d::default());

    // Directional light (sun)
    ctx.create().insert(DirectionalLight {
        direction: Vec3::new(-0.5, -1.0, -0.3),
        color: [1.0, 0.98, 0.95],
        intensity: 1.5,
    });

    // Point light
    ctx.create()
        .insert(Transform::from_xyz(4.0, 5.0, 4.0))
        .insert(PointLight { color: [1.0, 0.9, 0.7], intensity: 8.0, radius: 20.0 });

    spawn_ground_and_pyramid(&mut ctx.world);
}

fn spawn_ground_and_pyramid(world: &mut World) {
    // Ground
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0).with_scale(15.0),
        plane_mesh(),
        Material {
            base_color: [0.35, 0.35, 0.38, 1.0],
            metallic: 0.0,
            roughness: 0.9,
            ..Default::default()
        },
        RigidBody3d::fixed(),
        Collider3d::cuboid(15.0, 0.1, 15.0),
    ));

    // Ramp
    world.spawn((
        Transform {
            translation: Vec3::new(5.0, 0.5, 0.0),
            rotation: Quat::from_rotation_z(0.2),
            scale: Vec3::new(3.0, 0.1, 2.0),
        },
        cube_mesh(),
        Material {
            base_color: [0.5, 0.45, 0.4, 1.0],
            metallic: 0.0,
            roughness: 0.8,
            ..Default::default()
        },
        RigidBody3d::fixed(),
        Collider3d::cuboid(1.5, 0.05, 1.0),
    ));

    // Pyramid of cubes
    let layer_colors: [[f32; 4]; 4] = [
        [0.75, 0.2, 0.2, 1.0],
        [0.2, 0.7, 0.2, 1.0],
        [0.2, 0.3, 0.8, 1.0],
        [0.9, 0.8, 0.1, 1.0],
    ];

    let layers = [4, 3, 2, 1];
    for (layer, &count) in layers.iter().enumerate() {
        let y = 0.6 + layer as f32 * 1.05;
        let offset = -(count as f32 - 1.0) * 0.55;
        let color = layer_colors[layer];

        for col in 0..count {
            for row in 0..count {
                let x = offset + col as f32 * 1.1;
                let z = offset + row as f32 * 1.1;
                let entity = world.spawn((
                    Transform::from_xyz(x, y, z),
                    cube_mesh(),
                    Material {
                        base_color: color,
                        metallic: 0.3,
                        roughness: 0.4,
                        ..Default::default()
                    },
                    RigidBody3d::dynamic(),
                    Collider3d::cuboid(0.5, 0.5, 0.5).with_restitution(0.1).with_friction(0.8),
                ));
                world.tag(entity, "debris");
            }
        }
    }
}

fn shoot_sphere(ctx: &mut Context) {
    let fire = ctx.input.just_pressed(KeyCode::Space)
        || ctx.input.mouse_just_pressed(MouseButton::Left);
    if !fire {
        return;
    }

    let camera = ctx.world.named("camera");
    let cam_tf = *ctx.world.get::<Transform>(camera).unwrap();
    let cam_forward = cam_tf.rotation * Vec3::NEG_Z;
    let spawn_pos = cam_tf.translation + cam_forward * 2.0;
    let launch_speed = 18.0;

    let entity = ctx.world.spawn((
        Transform::from_xyz(spawn_pos.x, spawn_pos.y, spawn_pos.z),
        sphere_mesh(),
        Material {
            base_color: [0.9, 0.6, 0.1, 1.0],
            metallic: 0.7,
            roughness: 0.2,
            ..Default::default()
        },
        RigidBody3d::dynamic()
            .with_linear_velocity(cam_forward * launch_speed)
            .with_ccd(true),
        Collider3d::ball(0.5).with_restitution(0.4).with_density(3.0),
    ));
    ctx.world.tag(entity, "debris");
}

fn reset_scene(ctx: &mut Context) {
    if !ctx.input.just_pressed(KeyCode::KeyR) {
        return;
    }

    for e in ctx.world.tagged("debris") {
        ctx.world.despawn(e);
    }

    ctx.world.insert_resource(PhysicsWorld3d::new());
    spawn_ground_and_pyramid(&mut ctx.world);
}

fn toggle_debug_wireframes(ctx: &mut Context) {
    if ctx.input.just_pressed(KeyCode::F1) {
        let dbg = ctx.world.resource_mut::<DebugColliders3d>();
        dbg.enabled = !dbg.enabled;
    }
}

fn move_camera(ctx: &mut Context) {
    let dt = ctx.time.delta_secs();
    let speed = 8.0;

    let mut movement = Vec3::ZERO;
    if ctx.input.pressed(KeyCode::KeyW) { movement.z -= 1.0; }
    if ctx.input.pressed(KeyCode::KeyS) { movement.z += 1.0; }
    if ctx.input.pressed(KeyCode::KeyA) { movement.x -= 1.0; }
    if ctx.input.pressed(KeyCode::KeyD) { movement.x += 1.0; }
    if ctx.input.pressed(KeyCode::ShiftLeft) { movement.y -= 1.0; }
    if ctx.input.pressed(KeyCode::KeyQ) { movement.y += 1.0; }

    if movement == Vec3::ZERO {
        return;
    }
    let movement = movement.normalize() * speed * dt;

    let camera = ctx.world.named("camera");
    let tf = ctx.world.get_mut::<Transform>(camera).unwrap();
    let forward = tf.rotation * Vec3::NEG_Z;
    let right = tf.rotation * Vec3::X;
    let up = Vec3::Y;
    tf.translation += forward * movement.z + right * movement.x + up * movement.y;
}
