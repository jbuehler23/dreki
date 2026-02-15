//! Entity Hierarchies — solar system demo.
//!
//! Demonstrates parent/child relationships and automatic transform propagation.
//! Planets orbit the sun; moons orbit their planet — all via hierarchy.
//!
//! - **R** — despawn a random planet (children disappear too)
//! - **Space** — spawn a new planet with a moon
//! - **WASD** — move camera
//!
//! Run with: `cargo run -p dreki --example scene_hierarchy`

use dreki::prelude::*;

// ── Markers ──────────────────────────────────────────────────────────────

struct Sun;
struct Planet;
struct Moon;

// ── Orbit component ─────────────────────────────────────────────────────

struct Orbit {
    speed: f32,
}

// ── State ────────────────────────────────────────────────────────────────

struct PlanetCount(u32);

fn main() {
    env_logger::init();

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(HierarchyPlugin)
        .set_title("dreki — scene hierarchy (R despawn, Space spawn)")
        .insert_resource(ClearColor([0.02, 0.02, 0.06, 1.0]))
        .insert_resource(PlanetCount(0))
        .add_startup_system(setup)
        .add_system(orbit_system)
        .add_system(despawn_random_planet)
        .add_system(spawn_new_planet)
        .add_system(move_camera)
        .run();
}

fn setup(world: &mut World) {
    world.spawn((Transform::default(), Camera2d));

    // Sun — large yellow sprite at the center.
    let sun = world.spawn((
        Transform::default(),
        Sprite {
            color: Color::rgb(1.0, 0.9, 0.2),
            size: Vec2::new(60.0, 60.0),
            ..Default::default()
        },
        Sun,
    ));

    // Initial planets.
    let planet_configs = [
        (120.0, 1.0, Color::rgb(0.3, 0.5, 1.0), 20.0, 2.5),  // blue
        (200.0, 0.6, Color::rgb(0.9, 0.3, 0.2), 28.0, 1.8),  // red
        (300.0, 0.4, Color::rgb(0.2, 0.8, 0.3), 24.0, 3.0),  // green
        (400.0, 0.25, Color::rgb(0.8, 0.5, 0.1), 36.0, 2.0), // orange
    ];

    for &(dist, speed, color, size, moon_speed) in &planet_configs {
        world.resource_mut::<PlanetCount>().0 += 1;
        let idx = world.resource::<PlanetCount>().0;

        spawn_planet(world, sun, dist, speed, color, size, moon_speed, idx);
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
        Sprite {
            color,
            size: Vec2::new(size, size),
            ..Default::default()
        },
        Planet,
    ));

    // Moon — smaller sprite orbiting the planet.
    let moon_dist = size * 1.2 + 8.0;
    let moon_size = size * 0.35;
    let moon_angle = (seed as f32) * 1.7; // varied starting angle
    let _ = moon_angle;

    let moon_pivot = world.spawn_child(planet, (
        Transform::default(),
        Orbit { speed: moon_speed },
    ));

    world.spawn_child(moon_pivot, (
        Transform::from_xy(moon_dist, 0.0),
        Sprite {
            color: Color::rgb(0.7, 0.7, 0.7),
            size: Vec2::new(moon_size, moon_size),
            ..Default::default()
        },
        Moon,
    ));
}

fn orbit_system(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();

    let mut updates = Vec::new();
    world.query::<(&Orbit,)>(|entity, (orbit,)| {
        updates.push((entity, orbit.speed));
    });

    for (entity, speed) in updates {
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            let angle = speed * dt;
            transform.rotation *= Quat::from_rotation_z(angle);
        }
    }
}

fn despawn_random_planet(world: &mut World) {
    if !world.resource::<Input<KeyCode>>().just_pressed(KeyCode::KeyR) {
        return;
    }

    // Collect all planet entities.
    let mut planets = Vec::new();
    world.query::<(&Planet,)>(|entity, _| {
        planets.push(entity);
    });

    if planets.is_empty() {
        return;
    }

    // Pick one based on entity count as a simple "random" selection.
    let pick = world.entity_count() % planets.len();
    let victim = planets[pick];

    // Despawn recursively — moon children disappear too.
    // We need to despawn the orbit pivot (parent of the planet), which is
    // the planet's Parent. But the planet's parent is the orbit_pivot, and
    // the orbit_pivot's parent is the sun. We want to remove the orbit_pivot
    // subtree. Let's find the orbit_pivot.
    if let Some(parent) = world.get::<Parent>(victim) {
        let orbit_pivot = parent.0;
        world.despawn_recursive(orbit_pivot);
    } else {
        world.despawn_recursive(victim);
    }
}

fn spawn_new_planet(world: &mut World) {
    if !world.resource::<Input<KeyCode>>().just_pressed(KeyCode::Space) {
        return;
    }

    // Find the sun entity.
    let mut sun_entity = None;
    world.query::<(&Sun,)>(|entity, _| {
        sun_entity = Some(entity);
    });
    let Some(sun) = sun_entity else { return };

    let counter = &mut world.resource_mut::<PlanetCount>().0;
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

    spawn_planet(world, sun, distance, speed, color, size, moon_speed, idx);
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
