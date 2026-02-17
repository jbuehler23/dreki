//! Scene Switching — menu/game transitions with a persistent HUD.
//!
//! Demonstrates tagged scene loading and unloading. A HUD entity survives
//! all scene transitions because it is never tagged with a scene marker.
//!
//! - **Enter** — switch to game scene
//! - **Escape** — switch back to menu scene
//! - **WASD** — move camera
//!
//! Run with: `cargo run -p necs --example scene_switching`

use std::collections::HashMap;

use necs::prelude::*;
use necs::scene::SceneEntity as SceneEntityData;

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

// ── State ────────────────────────────────────────────────────────────────

struct ActiveScene(String);

fn main() {
    env_logger::init();

    Game::new("necs — scene switching (Enter game, Escape menu)")
        .resource(ClearColor([0.08, 0.08, 0.14, 1.0]))
        .resource(ActiveScene("menu".to_string()))
        .setup(setup)
        .update(switch_to_game)
        .update(switch_to_menu)
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
            make_scene_entity(0, 0.0, 60.0, 0.2, 0.8, 0.3, 200.0, 60.0, "Play"),
            make_scene_entity(1, 0.0, -20.0, 0.2, 0.3, 0.8, 200.0, 60.0, "Options"),
            make_scene_entity(2, 0.0, -100.0, 0.8, 0.2, 0.2, 200.0, 60.0, "Quit"),
            make_scene_entity(3, 0.0, 160.0, 0.9, 0.9, 0.2, 300.0, 20.0, "TitleBar"),
        ],
    }
}

fn game_scene_data() -> SceneData {
    SceneData {
        entities: vec![
            make_scene_entity(0, -100.0, 0.0, 0.3, 0.9, 0.3, 40.0, 40.0, "Player"),
            make_scene_entity(1, 150.0, 80.0, 0.9, 0.2, 0.2, 30.0, 30.0, "Enemy-1"),
            make_scene_entity(2, 200.0, -60.0, 0.9, 0.3, 0.1, 30.0, 30.0, "Enemy-2"),
            make_scene_entity(3, -50.0, -120.0, 0.8, 0.2, 0.3, 30.0, 30.0, "Enemy-3"),
            make_scene_entity(4, 0.0, -200.0, 0.3, 0.3, 0.35, 500.0, 30.0, "Ground"),
            make_scene_entity(5, -180.0, -100.0, 0.35, 0.3, 0.3, 80.0, 20.0, "Platform-L"),
            make_scene_entity(6, 120.0, -80.0, 0.35, 0.3, 0.3, 100.0, 20.0, "Platform-R"),
        ],
    }
}

fn setup(ctx: &mut Context) {
    ctx.spawn("camera").insert(Transform::default()).insert(Camera2d);

    // Persistent HUD — survives scene switches.
    ctx.create()
        .insert(Transform::from_xy(-350.0, 250.0))
        .insert(Sprite::new().color(Color::rgb(0.9, 0.9, 0.9)).size(20.0, 20.0))
        .tag("hud");

    // Load the initial menu scene.
    let registry = make_registry();
    registry.load_tagged(&mut ctx.world, &menu_scene_data(), "menu");
}

fn switch_to_game(ctx: &mut Context) {
    if !ctx.input.just_pressed(KeyCode::Enter) {
        return;
    }

    let current = &ctx.world.resource::<ActiveScene>().0;
    if current == "game" {
        return;
    }

    let registry = make_registry();
    registry.switch(&mut ctx.world, "menu", &game_scene_data(), "game");
    ctx.world.resource_mut::<ActiveScene>().0 = "game".to_string();
}

fn switch_to_menu(ctx: &mut Context) {
    if !ctx.input.just_pressed(KeyCode::Escape) {
        return;
    }

    let current = &ctx.world.resource::<ActiveScene>().0;
    if current == "menu" {
        return;
    }

    let registry = make_registry();
    registry.switch(&mut ctx.world, "game", &menu_scene_data(), "menu");
    ctx.world.resource_mut::<ActiveScene>().0 = "menu".to_string();
}

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
