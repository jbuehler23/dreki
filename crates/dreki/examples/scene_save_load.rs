//! Scene Save/Load — interactive round-trip with file I/O.
//!
//! Spawns colored sprites, lets you modify the scene, then save/load to disk.
//!
//! - **S** — save current scene to `/tmp/dreki_scene.json`
//! - **L** — clear and reload from that file
//! - **Space** — spawn a new random sprite
//! - **C** — clear all entities (except camera)
//! - **WASD** — move camera
//!
//! Run with: `cargo run -p dreki --example scene_save_load`

use dreki::prelude::*;

const SAVE_PATH: &str = "/tmp/dreki_scene.json";

// ── Serializable components ──────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SpriteInfo {
    r: f32,
    g: f32,
    b: f32,
    width: f32,
    height: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Label(String);

// ── Markers ──────────────────────────────────────────────────────────────

struct SceneEntity;

// ── State ────────────────────────────────────────────────────────────────

struct SpawnIndex(u32);

fn main() {
    env_logger::init();

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(HierarchyPlugin)
        .set_title("dreki — scene save/load (S save, L load, Space spawn, C clear)")
        .insert_resource(ClearColor([0.1, 0.1, 0.15, 1.0]))
        .insert_resource(SpawnIndex(0))
        .add_startup_system(setup)
        .add_system(save_on_s)
        .add_system(load_on_l)
        .add_system(spawn_on_space)
        .add_system(clear_on_c)
        .add_system(sync_sprites)
        .add_system(move_camera)
        .run();
}

fn make_registry() -> SceneRegistry {
    let mut registry = SceneRegistry::new();
    registry.register::<Transform>();
    registry.register::<SpriteInfo>();
    registry.register::<Label>();
    registry
}

fn setup(world: &mut World) {
    world.spawn((Transform::default(), Camera2d));

    // Spawn a few initial entities at varied positions.
    let configs = [
        (-200.0, 80.0, 1.0, 0.3, 0.3, 70.0, 70.0, "Red"),
        (-50.0, -60.0, 0.3, 1.0, 0.3, 90.0, 50.0, "Green"),
        (100.0, 40.0, 0.3, 0.3, 1.0, 60.0, 80.0, "Blue"),
        (200.0, -100.0, 1.0, 0.8, 0.2, 50.0, 50.0, "Yellow"),
        (-120.0, 120.0, 0.8, 0.2, 0.8, 45.0, 65.0, "Purple"),
    ];

    for &(x, y, r, g, b, w, h, label) in &configs {
        spawn_scene_entity(world, x, y, r, g, b, w, h, label);
    }

    // Demonstrate hierarchy: spawn a parent with a child.
    let parent = world.spawn((
        Transform::from_xy(0.0, 150.0),
        SpriteInfo { r: 1.0, g: 0.5, b: 0.0, width: 80.0, height: 80.0 },
        Sprite {
            color: Color::rgb(1.0, 0.5, 0.0),
            size: Vec2::new(80.0, 80.0),
            ..Default::default()
        },
        Label("Parent".into()),
        SceneEntity,
    ));
    world.spawn_child(parent, (
        Transform::from_xy(60.0, 0.0),
        SpriteInfo { r: 0.0, g: 0.8, b: 0.8, width: 40.0, height: 40.0 },
        Sprite {
            color: Color::rgb(0.0, 0.8, 0.8),
            size: Vec2::new(40.0, 40.0),
            ..Default::default()
        },
        Label("Child".into()),
        SceneEntity,
    ));
}

fn spawn_scene_entity(
    world: &mut World,
    x: f32, y: f32,
    r: f32, g: f32, b: f32,
    w: f32, h: f32,
    label: &str,
) -> Entity {
    world.spawn((
        Transform::from_xy(x, y),
        SpriteInfo { r, g, b, width: w, height: h },
        Sprite {
            color: Color::rgb(r, g, b),
            size: Vec2::new(w, h),
            ..Default::default()
        },
        Label(label.to_string()),
        SceneEntity,
    ))
}

fn save_on_s(world: &mut World) {
    if !world.resource::<Input<KeyCode>>().just_pressed(KeyCode::KeyS) {
        return;
    }

    let registry = make_registry();
    save_scene_to_file(world, &registry, SAVE_PATH);
    log::info!("Scene saved to {}", SAVE_PATH);
}

fn load_on_l(world: &mut World) {
    if !world.resource::<Input<KeyCode>>().just_pressed(KeyCode::KeyL) {
        return;
    }

    if !std::path::Path::new(SAVE_PATH).exists() {
        log::warn!("No save file at {} — press S first", SAVE_PATH);
        return;
    }

    // Despawn all scene entities, keep the camera.
    let mut to_despawn = Vec::new();
    world.query::<(&SceneEntity,)>(|entity, _| {
        to_despawn.push(entity);
    });
    for e in to_despawn {
        world.despawn_recursive(e);
    }

    let registry = make_registry();
    let loaded = load_scene_from_file(world, &registry, SAVE_PATH);
    log::info!("Loaded {} entities from {}", loaded.len(), SAVE_PATH);
}

fn spawn_on_space(world: &mut World) {
    if !world.resource::<Input<KeyCode>>().just_pressed(KeyCode::Space) {
        return;
    }

    let idx = &mut world.resource_mut::<SpawnIndex>().0;
    *idx += 1;
    let i = *idx;

    // Vary position and color.
    let x = ((i * 97) % 500) as f32 - 250.0;
    let y = ((i * 53) % 400) as f32 - 200.0;
    let colors = [
        (1.0, 0.4, 0.4),
        (0.4, 1.0, 0.4),
        (0.4, 0.4, 1.0),
        (1.0, 1.0, 0.4),
        (1.0, 0.4, 1.0),
        (0.4, 1.0, 1.0),
    ];
    let (r, g, b) = colors[(i as usize) % colors.len()];
    let size = 30.0 + (i % 5) as f32 * 15.0;

    spawn_scene_entity(world, x, y, r, g, b, size, size, &format!("Spawn-{}", i));
}

fn clear_on_c(world: &mut World) {
    if !world.resource::<Input<KeyCode>>().just_pressed(KeyCode::KeyC) {
        return;
    }

    let mut to_despawn = Vec::new();
    world.query::<(&SceneEntity,)>(|entity, _| {
        to_despawn.push(entity);
    });
    // Only despawn root entities to avoid double-despawning children.
    let roots: Vec<_> = to_despawn
        .iter()
        .copied()
        .filter(|&e| world.get::<Parent>(e).is_none())
        .collect();
    for e in roots {
        world.despawn_recursive(e);
    }
}

/// After loading from file, entities have SpriteInfo but no Sprite.
/// This system adds the visual Sprite component based on SpriteInfo.
fn sync_sprites(world: &mut World) {
    let mut needs_sprite = Vec::new();
    world.query::<(&SpriteInfo,)>(|entity, (info,)| {
        needs_sprite.push((
            entity,
            info.r, info.g, info.b,
            info.width, info.height,
        ));
    });

    for (entity, r, g, b, w, h) in needs_sprite {
        // Only add Sprite if the entity doesn't have one yet.
        if world.get::<Sprite>(entity).is_some() {
            continue;
        }
        world.insert(entity, Sprite {
            color: Color::rgb(r, g, b),
            size: Vec2::new(w, h),
            ..Default::default()
        });
        // Also mark as scene entity for cleanup.
        if world.get::<SceneEntity>(entity).is_none() {
            world.insert(entity, SceneEntity);
        }
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
