//! Editor 3D — demonstrates the in-engine editor with a 3D scene.
//!
//! Press **F12** to toggle the editor overlay. Use the hierarchy to select
//! entities, then drag transform values in the inspector to move objects
//! in real time.
//!
//! Run with:
//!     cargo run -p necs --example editor_3d --features "editor,render3d"

use necs::prelude::*;

fn main() {
    env_logger::init();

    Game::new("necs — editor 3d demo")
        .resource(ClearColor([0.08, 0.08, 0.12, 1.0]))
        .resource(AmbientLight {
            color: [0.15, 0.15, 0.2],
            intensity: 0.3,
        })
        .setup(setup)
        .update(orbit_camera)
        .run();
}

/// Marker for the orbiting camera.
struct OrbitCam {
    angle: f32,
    radius: f32,
    height: f32,
}

fn setup(ctx: &mut Context) {
    // Camera — orbits around the origin.
    ctx.spawn("camera")
        .insert(Transform::from_xyz(0.0, 5.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y))
        .insert(Camera3d::default())
        .insert(OrbitCam { angle: 0.0, radius: 8.0, height: 5.0 });

    // Ground plane
    ctx.spawn("ground")
        .insert(Transform::from_xyz(0.0, 0.0, 0.0).with_scale(10.0))
        .insert(Mesh3d::plane())
        .insert(Material {
            base_color: [0.3, 0.3, 0.3, 1.0],
            roughness: 0.9,
            ..Default::default()
        })
        .tag("environment");

    // Center cube — edit its transform in the inspector!
    ctx.spawn("center_cube")
        .insert(Transform::from_xyz(0.0, 0.5, 0.0))
        .insert(Mesh3d::cube())
        .insert(Material {
            base_color: [0.8, 0.2, 0.2, 1.0],
            metallic: 0.3,
            roughness: 0.4,
            ..Default::default()
        })
        .tag("object");

    // Sphere to the left
    ctx.spawn("metal_sphere")
        .insert(Transform::from_xyz(-2.5, 0.7, 0.0))
        .insert(Mesh3d::sphere())
        .insert(Material {
            base_color: [0.9, 0.9, 0.9, 1.0],
            metallic: 0.95,
            roughness: 0.05,
            ..Default::default()
        })
        .tag("object");

    // Cylinder to the right
    ctx.spawn("cylinder")
        .insert(Transform::from_xyz(2.5, 0.5, 0.0))
        .insert(Mesh3d::cylinder())
        .insert(Material {
            base_color: [0.2, 0.6, 0.9, 1.0],
            roughness: 0.6,
            ..Default::default()
        })
        .tag("object");

    // A small stack of cubes — hierarchy of children.
    let stack = ctx.spawn("cube_stack")
        .insert(Transform::from_xyz(0.0, 0.5, -3.0))
        .insert(Mesh3d::cube())
        .insert(Material {
            base_color: [0.4, 0.8, 0.3, 1.0],
            roughness: 0.5,
            ..Default::default()
        })
        .id();

    ctx.world.spawn_child(stack, (
        Transform::from_xyz(0.0, 1.0, 0.0),
        Mesh3d::cube(),
        Material {
            base_color: [0.6, 0.9, 0.4, 1.0],
            roughness: 0.5,
            ..Default::default()
        },
    ));
    ctx.world.spawn_child(stack, (
        Transform::from_xyz(0.0, 2.0, 0.0),
        Mesh3d::cube(),
        Material {
            base_color: [0.8, 1.0, 0.5, 1.0],
            roughness: 0.5,
            ..Default::default()
        },
    ));

    // Lights
    ctx.spawn("sun")
        .insert(DirectionalLight {
            direction: Vec3::new(-0.5, -1.0, -0.3).normalize(),
            color: [1.0, 0.95, 0.9],
            intensity: 1.0,
        })
        .tag("light");

    ctx.spawn("fill_light")
        .insert(Transform::from_xyz(3.0, 4.0, 2.0))
        .insert(PointLight {
            color: [0.5, 0.7, 1.0],
            intensity: 50.0,
            radius: 20.0,
        })
        .tag("light");

    log::info!("Press F12 to open the editor. Drag transform values to move objects.");
    log::info!("WASD to orbit camera, Q/E for height.");
}

fn orbit_camera(ctx: &mut Context) {
    let dt = ctx.time.delta_secs();
    let camera = ctx.world.named("camera");

    let orbit = ctx.world.get_mut::<OrbitCam>(camera).unwrap();
    if ctx.input.pressed(KeyCode::KeyA) { orbit.angle -= 1.5 * dt; }
    if ctx.input.pressed(KeyCode::KeyD) { orbit.angle += 1.5 * dt; }
    if ctx.input.pressed(KeyCode::KeyW) { orbit.radius = (orbit.radius - 3.0 * dt).max(2.0); }
    if ctx.input.pressed(KeyCode::KeyS) { orbit.radius = (orbit.radius + 3.0 * dt).min(20.0); }
    if ctx.input.pressed(KeyCode::KeyQ) { orbit.height += 3.0 * dt; }
    if ctx.input.pressed(KeyCode::KeyE) { orbit.height = (orbit.height - 3.0 * dt).max(1.0); }
    let angle = orbit.angle;
    let radius = orbit.radius;
    let height = orbit.height;

    let tf = ctx.world.get_mut::<Transform>(camera).unwrap();
    tf.translation = Vec3::new(angle.cos() * radius, height, angle.sin() * radius);
    *tf = tf.looking_at(Vec3::ZERO, Vec3::Y);
}
