//! Scene Save/Load — interactive round-trip with file I/O.
//!
//! Spawns colored sprites, lets you modify the scene, then save/load to disk.
//!
//! - **F5** — save current scene to `/tmp/necs_scene.json`
//! - **F9** — clear and reload from that file
//! - **Space** — spawn a new random sprite
//! - **X** — clear all entities (except camera)
//! - **WASD** — move camera
//!
//! Run with: `cargo run -p necs --example scene_save_load`

use necs::prelude::*;

const SAVE_PATH: &str = "/tmp/necs_scene.json";

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

// ── State ────────────────────────────────────────────────────────────────

struct SpawnIndex(u32);

fn main() {
    env_logger::init();

    Game::new("necs — scene save/load (F5 save, F9 load, Space spawn, X clear)")
        .resource(ClearColor([0.1, 0.1, 0.15, 1.0]))
        .resource(SpawnIndex(0))
        .setup(setup)
        .update(save_on_s)
        .update(load_on_l)
        .update(spawn_on_space)
        .update(clear_on_c)
        .update(sync_sprites)
        .update(move_camera)
        .run();
}

fn make_registry() -> SceneRegistry {
    let mut registry = SceneRegistry::new();
    registry.register::<Transform>();
    registry.register::<SpriteInfo>();
    registry.register::<Label>();
    registry
}

fn setup(ctx: &mut Context) {
    ctx.spawn("camera").insert(Transform::default()).insert(Camera2d);

    // Spawn a few initial entities at varied positions.
    let configs = [
        (-200.0, 80.0, 1.0, 0.3, 0.3, 70.0, 70.0, "Red"),
        (-50.0, -60.0, 0.3, 1.0, 0.3, 90.0, 50.0, "Green"),
        (100.0, 40.0, 0.3, 0.3, 1.0, 60.0, 80.0, "Blue"),
        (200.0, -100.0, 1.0, 0.8, 0.2, 50.0, 50.0, "Yellow"),
        (-120.0, 120.0, 0.8, 0.2, 0.8, 45.0, 65.0, "Purple"),
    ];

    for &(x, y, r, g, b, w, h, label) in &configs {
        spawn_scene_entity(&mut ctx.world, x, y, r, g, b, w, h, label);
    }

    // Demonstrate hierarchy: spawn a parent with a child.
    let parent = ctx.world.spawn((
        Transform::from_xy(0.0, 150.0),
        SpriteInfo { r: 1.0, g: 0.5, b: 0.0, width: 80.0, height: 80.0 },
        Sprite::new().color(Color::rgb(1.0, 0.5, 0.0)).size(80.0, 80.0),
        Label("Parent".into()),
    ));
    ctx.world.tag(parent, "scene-entity");

    ctx.world.spawn_child(parent, (
        Transform::from_xy(60.0, 0.0),
        SpriteInfo { r: 0.0, g: 0.8, b: 0.8, width: 40.0, height: 40.0 },
        Sprite::new().color(Color::rgb(0.0, 0.8, 0.8)).size(40.0, 40.0),
        Label("Child".into()),
    ));
}

fn spawn_scene_entity(
    world: &mut World,
    x: f32, y: f32,
    r: f32, g: f32, b: f32,
    w: f32, h: f32,
    label: &str,
) -> Entity {
    let entity = world.spawn((
        Transform::from_xy(x, y),
        SpriteInfo { r, g, b, width: w, height: h },
        Sprite::new().color(Color::rgb(r, g, b)).size(w, h),
        Label(label.to_string()),
    ));
    world.tag(entity, "scene-entity");
    entity
}

fn save_on_s(ctx: &mut Context) {
    if !ctx.input.just_pressed(KeyCode::F5) {
        return;
    }

    let registry = make_registry();
    registry.save_to_file(&ctx.world, SAVE_PATH);
    log::info!("Scene saved to {}", SAVE_PATH);
}

fn load_on_l(ctx: &mut Context) {
    if !ctx.input.just_pressed(KeyCode::F9) {
        return;
    }

    if !std::path::Path::new(SAVE_PATH).exists() {
        log::warn!("No save file at {} — press S first", SAVE_PATH);
        return;
    }

    // Despawn all scene entities, keep the camera.
    for e in ctx.world.tagged("scene-entity") {
        ctx.world.despawn_recursive(e);
    }

    let registry = make_registry();
    let loaded = registry.load_from_file(&mut ctx.world, SAVE_PATH);
    log::info!("Loaded {} entities from {}", loaded.len(), SAVE_PATH);
}

fn spawn_on_space(ctx: &mut Context) {
    if !ctx.input.just_pressed(KeyCode::Space) {
        return;
    }

    let idx = &mut ctx.world.resource_mut::<SpawnIndex>().0;
    *idx += 1;
    let i = *idx;

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

    spawn_scene_entity(&mut ctx.world, x, y, r, g, b, size, size, &format!("Spawn-{}", i));
}

fn clear_on_c(ctx: &mut Context) {
    if !ctx.input.just_pressed(KeyCode::KeyX) {
        return;
    }

    let to_despawn = ctx.world.tagged("scene-entity");
    let roots: Vec<_> = to_despawn
        .iter()
        .copied()
        .filter(|&e| ctx.world.get::<Parent>(e).is_none())
        .collect();
    for e in roots {
        ctx.world.despawn_recursive(e);
    }
}

/// After loading from file, entities have SpriteInfo but no Sprite.
/// This system adds the visual Sprite component based on SpriteInfo.
fn sync_sprites(ctx: &mut Context) {
    let mut needs_sprite = Vec::new();
    ctx.world.query::<(&SpriteInfo,)>(|entity, (info,)| {
        needs_sprite.push((entity, info.r, info.g, info.b, info.width, info.height));
    });

    for (entity, r, g, b, w, h) in needs_sprite {
        if ctx.world.get::<Sprite>(entity).is_some() {
            continue;
        }
        ctx.world.insert(entity, Sprite::new().color(Color::rgb(r, g, b)).size(w, h));
        // Tag loaded entities for cleanup.
        if !ctx.world.tagged("scene-entity").contains(&entity) {
            ctx.world.tag(entity, "scene-entity");
        }
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
