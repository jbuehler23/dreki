//! 3D Physics — interactive cube pyramid.
//!
//! Press Space to launch a sphere at the pyramid. Press R to reset.
//! WASD + Space/Shift to move the camera. Mouse click also launches a sphere.

use kera::prelude::*;

fn main() {
    env_logger::init();

    App::new()
        .add_plugins(DefaultPlugins)
        .set_title("kera — 3d physics (space to shoot, F1 wireframes)")
        .insert_resource(ClearColor([0.08, 0.08, 0.12, 1.0]))
        .insert_resource(AmbientLight {
            color: [1.0, 1.0, 1.0],
            intensity: 0.1,
        })
        .add_plugins(Physics3dPlugin::new())
        .add_startup_system(setup)
        .add_system(shoot_sphere)
        .add_system(reset_scene)
        .add_system(move_camera)
        .add_system(toggle_debug_wireframes)
        .run();
}

/// Marker for objects that get cleared on reset.
struct Debris;

fn setup(world: &mut World) {
    // Camera
    world.spawn((
        Transform::from_xyz(0.0, 6.0, 14.0).looking_at(Vec3::new(0.0, 2.0, 0.0), Vec3::Y),
        Camera3d::default(),
    ));

    // Directional light (sun)
    world.spawn_one(DirectionalLight {
        direction: Vec3::new(-0.5, -1.0, -0.3),
        color: [1.0, 0.98, 0.95],
        intensity: 1.5,
    });

    // Point light for extra fill
    world.spawn((
        Transform::from_xyz(4.0, 5.0, 4.0),
        PointLight {
            color: [1.0, 0.9, 0.7],
            intensity: 8.0,
            radius: 20.0,
        },
    ));

    spawn_ground_and_pyramid(world);
}

fn spawn_ground_and_pyramid(world: &mut World) {
    // Ground: visual plane at y=0, collider centered at y=0 with thin half-height.
    // Objects rest at y = half_height, so cubes (half-extent 0.5) rest at y=0.5.
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

    // A ramp for visual interest
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

    // Pyramid of cubes (4-3-2-1 layers)
    let layer_colors: [[f32; 4]; 4] = [
        [0.75, 0.2, 0.2, 1.0],   // red bottom
        [0.2, 0.7, 0.2, 1.0],    // green
        [0.2, 0.3, 0.8, 1.0],    // blue
        [0.9, 0.8, 0.1, 1.0],    // yellow top
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
                world.spawn((
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
                    Debris,
                ));
            }
        }
    }
}

fn shoot_sphere(world: &mut World) {
    let fire = world.resource::<Input<KeyCode>>().just_pressed(KeyCode::Space)
        || world.resource::<Input<MouseButton>>().just_pressed(MouseButton::Left);
    if !fire {
        return;
    }

    // Get camera transform to shoot from.
    let mut cam_pos = Vec3::ZERO;
    let mut cam_forward = Vec3::NEG_Z;
    world.query_single::<(&Transform,), Camera3d>(|_e, (tf,)| {
        cam_pos = tf.translation;
        cam_forward = tf.rotation * Vec3::NEG_Z;
    });

    let spawn_pos = cam_pos + cam_forward * 2.0;
    let launch_speed = 18.0;

    world.spawn((
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
        Debris,
    ));
}

fn reset_scene(world: &mut World) {
    if !world.resource::<Input<KeyCode>>().just_pressed(KeyCode::KeyR) {
        return;
    }

    // Despawn all debris.
    let mut to_despawn = Vec::new();
    world.query::<(&Debris,)>(|entity, _| {
        to_despawn.push(entity);
    });
    for e in to_despawn {
        world.despawn(e);
    }

    // Also need a fresh PhysicsWorld since old handles are now stale.
    world.insert_resource(PhysicsWorld3d::new());

    spawn_ground_and_pyramid(world);
}

fn toggle_debug_wireframes(world: &mut World) {
    if world.resource::<Input<KeyCode>>().just_pressed(KeyCode::F1) {
        let dbg = world.resource_mut::<DebugColliders3d>();
        dbg.enabled = !dbg.enabled;
    }
}

fn move_camera(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let input = world.resource::<Input<KeyCode>>();
    let speed = 8.0;

    let mut movement = Vec3::ZERO;
    if input.pressed(KeyCode::KeyW) { movement.z -= 1.0; }
    if input.pressed(KeyCode::KeyS) { movement.z += 1.0; }
    if input.pressed(KeyCode::KeyA) { movement.x -= 1.0; }
    if input.pressed(KeyCode::KeyD) { movement.x += 1.0; }
    if input.pressed(KeyCode::ShiftLeft) { movement.y -= 1.0; }

    // Don't use Space for camera up — it's used for shooting.
    if input.pressed(KeyCode::KeyQ) { movement.y += 1.0; }

    if movement == Vec3::ZERO {
        return;
    }
    let movement = movement.normalize() * speed * dt;

    world.query_single::<(&mut Transform,), Camera3d>(|_e, (tf,)| {
        let forward = tf.rotation * Vec3::NEG_Z;
        let right = tf.rotation * Vec3::X;
        let up = Vec3::Y;
        tf.translation += forward * movement.z + right * movement.x + up * movement.y;
    });
}
