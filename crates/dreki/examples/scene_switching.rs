//! Scene Switching — menu/game transitions with a persistent HUD.
//!
//! Demonstrates tagged scene loading and unloading. A HUD entity survives
//! all scene transitions because it is never tagged with a scene marker.
//!
//! - **Enter** — switch to game scene
//! - **Escape** — switch back to menu scene
//! - **WASD** — move camera
//!
//! Run with: `cargo run -p dreki --example scene_switching`

use std::collections::HashMap;

use dreki::prelude::*;
use dreki::scene::SceneEntity as SceneEntityData;

// ── Serializable components for scene data ───────────────────────────────

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

struct Hud;

// ── State ────────────────────────────────────────────────────────────────

struct ActiveScene(String);

fn main() {
    env_logger::init();

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(HierarchyPlugin)
        .set_title("dreki — scene switching (Enter game, Escape menu)")
        .insert_resource(ClearColor([0.08, 0.08, 0.14, 1.0]))
        .insert_resource(ActiveScene("menu".to_string()))
        .add_startup_system(setup)
        .add_system(switch_to_game)
        .add_system(switch_to_menu)
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

fn make_scene_entity(
    id: u32,
    x: f32, y: f32,
    r: f32, g: f32, b: f32,
    w: f32, h: f32,
    label: &str,
) -> SceneEntityData {
    let mut components = HashMap::new();
    components.insert(
        "Transform".to_string(),
        serde_json::to_value(Transform::from_xy(x, y)).unwrap(),
    );
    components.insert(
        "SpriteInfo".to_string(),
        serde_json::to_value(SpriteInfo { r, g, b, width: w, height: h }).unwrap(),
    );
    components.insert(
        "Label".to_string(),
        serde_json::to_value(Label(label.to_string())).unwrap(),
    );
    SceneEntityData {
        id,
        components,
        children: vec![],
    }
}

fn menu_scene_data() -> SceneData {
    SceneData {
        entities: vec![
            // "Play" button — large green rectangle.
            make_scene_entity(0, 0.0, 60.0, 0.2, 0.8, 0.3, 200.0, 60.0, "Play"),
            // "Options" button — blue rectangle.
            make_scene_entity(1, 0.0, -20.0, 0.2, 0.3, 0.8, 200.0, 60.0, "Options"),
            // "Quit" button — red rectangle.
            make_scene_entity(2, 0.0, -100.0, 0.8, 0.2, 0.2, 200.0, 60.0, "Quit"),
            // Decorative sprite — title accent.
            make_scene_entity(3, 0.0, 160.0, 0.9, 0.9, 0.2, 300.0, 20.0, "TitleBar"),
        ],
    }
}

fn game_scene_data() -> SceneData {
    SceneData {
        entities: vec![
            // Player.
            make_scene_entity(0, -100.0, 0.0, 0.3, 0.9, 0.3, 40.0, 40.0, "Player"),
            // Enemies.
            make_scene_entity(1, 150.0, 80.0, 0.9, 0.2, 0.2, 30.0, 30.0, "Enemy-1"),
            make_scene_entity(2, 200.0, -60.0, 0.9, 0.3, 0.1, 30.0, 30.0, "Enemy-2"),
            make_scene_entity(3, -50.0, -120.0, 0.8, 0.2, 0.3, 30.0, 30.0, "Enemy-3"),
            // Terrain.
            make_scene_entity(4, 0.0, -200.0, 0.3, 0.3, 0.35, 500.0, 30.0, "Ground"),
            make_scene_entity(5, -180.0, -100.0, 0.35, 0.3, 0.3, 80.0, 20.0, "Platform-L"),
            make_scene_entity(6, 120.0, -80.0, 0.35, 0.3, 0.3, 100.0, 20.0, "Platform-R"),
        ],
    }
}

fn setup(world: &mut World) {
    world.spawn((Transform::default(), Camera2d));

    // Persistent HUD — a small indicator in the corner that survives scene switches.
    world.spawn((
        Transform::from_xy(-350.0, 250.0),
        Sprite {
            color: Color::rgb(0.9, 0.9, 0.9),
            size: Vec2::new(20.0, 20.0),
            ..Default::default()
        },
        Hud,
    ));

    // Load the initial menu scene.
    let registry = make_registry();
    load_scene_tagged(world, &registry, &menu_scene_data(), "menu");
}

fn switch_to_game(world: &mut World) {
    if !world.resource::<Input<KeyCode>>().just_pressed(KeyCode::Enter) {
        return;
    }

    let current = &world.resource::<ActiveScene>().0;
    if current == "game" {
        return;
    }

    let registry = make_registry();
    switch_scene(world, &registry, "menu", &game_scene_data(), "game");
    world.resource_mut::<ActiveScene>().0 = "game".to_string();
}

fn switch_to_menu(world: &mut World) {
    if !world.resource::<Input<KeyCode>>().just_pressed(KeyCode::Escape) {
        return;
    }

    let current = &world.resource::<ActiveScene>().0;
    if current == "menu" {
        return;
    }

    let registry = make_registry();
    switch_scene(world, &registry, "game", &menu_scene_data(), "menu");
    world.resource_mut::<ActiveScene>().0 = "menu".to_string();
}

/// After loading scene data, entities have SpriteInfo but no visual Sprite.
/// This system adds the Sprite component to make them visible.
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
        if world.get::<Sprite>(entity).is_some() {
            continue;
        }
        world.insert(entity, Sprite {
            color: Color::rgb(r, g, b),
            size: Vec2::new(w, h),
            ..Default::default()
        });
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
