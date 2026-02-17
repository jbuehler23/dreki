//! Entity Hierarchies — solar system demo.
//!
//! Demonstrates parent/child relationships and automatic transform propagation.
//! Planets orbit the sun; moons orbit their planet — all via hierarchy.
//!
//! - **R** — despawn a random planet (children disappear too)
//! - **Space** — spawn a new planet with a moon
//! - **WASD** — move camera
//!
//! Run with: `cargo run -p necs --example scene_hierarchy`

use necs::prelude::*;

// ── Orbit component ─────────────────────────────────────────────────────

struct Orbit {
    speed: f32,
}

// ── State ────────────────────────────────────────────────────────────────

struct PlanetCount(u32);

fn main() {
    env_logger::init();

    Game::new("necs — scene hierarchy (R despawn, Space spawn)")
        .resource(ClearColor([0.02, 0.02, 0.06, 1.0]))
        .resource(PlanetCount(0))
        .setup(setup)
        .update(orbit_system)
        .update(despawn_random_planet)
        .update(spawn_new_planet)
        .update(move_camera)
        .run();
}

fn setup(ctx: &mut Context) {
    ctx.spawn("camera").insert(Transform::default()).insert(Camera2d);

    // Sun — large yellow sprite at the center.
    let sun = ctx.create()
        .insert(Transform::default())
        .insert(Sprite::new().color(Color::rgb(1.0, 0.9, 0.2)).size(60.0, 60.0))
        .tag("sun")
        .id();

    // Initial planets.
    let planet_configs = [
        (120.0, 1.0, Color::rgb(0.3, 0.5, 1.0), 20.0, 2.5),  // blue
        (200.0, 0.6, Color::rgb(0.9, 0.3, 0.2), 28.0, 1.8),  // red
        (300.0, 0.4, Color::rgb(0.2, 0.8, 0.3), 24.0, 3.0),  // green
        (400.0, 0.25, Color::rgb(0.8, 0.5, 0.1), 36.0, 2.0), // orange
    ];

    for &(dist, speed, color, size, moon_speed) in &planet_configs {
        ctx.world.resource_mut::<PlanetCount>().0 += 1;
        let idx = ctx.world.resource::<PlanetCount>().0;

        spawn_planet(&mut ctx.world, sun, dist, speed, color, size, moon_speed, idx);
    }
}

fn spawn_planet(
    world: &mut World,
    sun: Entity,
    distance: f32,
    speed: f32,
    color: Color,
    size: f32,
    moon_speed: f32,
    seed: u32,
) {
    // The planet's parent transform controls the orbit angle.
    // By rotating the parent, the child (the visible sprite) orbits.
    let orbit_pivot = world.spawn_child(sun, (
        Transform::default(),
        Orbit { speed },
    ));

    // The visible planet sprite, offset from the pivot.
    let planet = world.spawn_child(orbit_pivot, (
        Transform::from_xy(distance, 0.0),
        Sprite::new().color(color).size(size, size),
    ));
    world.tag(planet, "planet");

    // Moon — smaller sprite orbiting the planet.
    let moon_dist = size * 1.2 + 8.0;
    let moon_size = size * 0.35;
    let _moon_angle = (seed as f32) * 1.7; // varied starting angle

    let moon_pivot = world.spawn_child(planet, (
        Transform::default(),
        Orbit { speed: moon_speed },
    ));

    world.spawn_child(moon_pivot, (
        Transform::from_xy(moon_dist, 0.0),
        Sprite::new().color(Color::rgb(0.7, 0.7, 0.7)).size(moon_size, moon_size),
    ));
}

fn orbit_system(ctx: &mut Context) {
    let dt = ctx.time.delta_secs();

    let mut updates = Vec::new();
    ctx.world.query::<(&Orbit,)>(|entity, (orbit,)| {
        updates.push((entity, orbit.speed));
    });

    for (entity, speed) in updates {
        if let Some(transform) = ctx.world.get_mut::<Transform>(entity) {
            let angle = speed * dt;
            transform.rotation *= Quat::from_rotation_z(angle);
        }
    }
}

fn despawn_random_planet(ctx: &mut Context) {
    if !ctx.input.just_pressed(KeyCode::KeyR) {
        return;
    }

    let planets = ctx.world.tagged("planet");
    if planets.is_empty() {
        return;
    }

    // Pick one based on entity count as a simple "random" selection.
    let pick = ctx.world.entity_count() % planets.len();
    let victim = planets[pick];

    // Despawn recursively — moon children disappear too.
    // Remove the orbit_pivot (parent of the planet) subtree.
    if let Some(parent) = ctx.world.get::<Parent>(victim) {
        let orbit_pivot = parent.0;
        ctx.world.despawn_recursive(orbit_pivot);
    } else {
        ctx.world.despawn_recursive(victim);
    }
}

fn spawn_new_planet(ctx: &mut Context) {
    if !ctx.input.just_pressed(KeyCode::Space) {
        return;
    }

    let sun = ctx.world.tagged("sun");
    let Some(&sun) = sun.first() else { return };

    let counter = &mut ctx.world.resource_mut::<PlanetCount>().0;
    *counter += 1;
    let idx = *counter;

    // Vary properties based on count.
    let distance = 100.0 + (idx as f32) * 50.0;
    let speed = 0.8 / (idx as f32).sqrt();
    let colors = [
        Color::rgb(0.6, 0.2, 0.8),
        Color::rgb(0.2, 0.7, 0.8),
        Color::rgb(0.9, 0.8, 0.2),
        Color::rgb(0.8, 0.3, 0.5),
        Color::rgb(0.3, 0.9, 0.5),
    ];
    let color = colors[(idx as usize) % colors.len()];
    let size = 18.0 + (idx % 5) as f32 * 6.0;
    let moon_speed = 1.5 + (idx % 3) as f32 * 0.8;

    spawn_planet(&mut ctx.world, sun, distance, speed, color, size, moon_speed, idx);
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
