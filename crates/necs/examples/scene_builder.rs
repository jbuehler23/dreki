//! Scene Builder — programmatic scene construction with Templates.
//!
//! Demonstrates defining reusable entity templates, composing them into named
//! scenes, and transitioning between scenes via `SceneManager`.
//!
//! - **Enter** — switch to game scene
//! - **Escape** — switch to menu scene
//! - **WASD** — move camera
//!
//! Run with: `cargo run -p necs --example scene_builder`

use necs::prelude::*;

fn main() {
    env_logger::init();

    let menu_scene = SceneBuilder::new("menu")
        .add(
            Template::new()
                .with(Transform::from_xy(0.0, 100.0))
                .with(Sprite::new().color(Color::rgb(0.2, 0.8, 0.3)).size(200.0, 60.0))
                .tag("button"),
        )
        .add(
            Template::new()
                .with(Transform::from_xy(0.0, 20.0))
                .with(Sprite::new().color(Color::rgb(0.2, 0.3, 0.8)).size(200.0, 60.0))
                .tag("button"),
        )
        .add(
            Template::new()
                .with(Transform::from_xy(0.0, -60.0))
                .with(Sprite::new().color(Color::rgb(0.8, 0.2, 0.2)).size(200.0, 60.0))
                .tag("button"),
        )
        .on_enter(|_ctx| {
            log::info!("Entered menu scene");
        })
        .on_exit(|_ctx| {
            log::info!("Left menu scene");
        });

    // Reusable enemy template.
    let enemy = Template::new()
        .with(Sprite::new().color(Color::RED).size(30.0, 30.0))
        .tag("enemy");

    // Player template with a child (weapon).
    let player = Template::new()
        .name("player")
        .with(Transform::from_xy(0.0, 0.0))
        .with(Sprite::new().color(Color::GREEN).size(40.0, 40.0))
        .child(
            Template::new()
                .with(Transform::from_xy(25.0, 0.0))
                .with(Sprite::new().color(Color::WHITE).size(12.0, 6.0)),
        );

    let game_scene = SceneBuilder::new("game")
        .add(player)
        .add(enemy.clone().with(Transform::from_xy(150.0, 80.0)))
        .add(enemy.clone().with(Transform::from_xy(200.0, -60.0)))
        .add(enemy.with(Transform::from_xy(-120.0, -90.0)))
        .add(
            Template::new()
                .with(Transform::from_xy(0.0, -180.0))
                .with(Sprite::new().color(Color::rgb(0.3, 0.3, 0.35)).size(500.0, 30.0))
                .tag("ground"),
        )
        .on_enter(|_ctx| {
            log::info!("Entered game scene");
        })
        .on_exit(|_ctx| {
            log::info!("Left game scene");
        });

    Game::new("necs — scene builder (Enter=game, Escape=menu)")
        .resource(ClearColor([0.08, 0.08, 0.14, 1.0]))
        .plugin(
            Scenes::new()
                .add(menu_scene)
                .add(game_scene)
                .start("menu"),
        )
        .setup(setup)
        .update(switch_scenes)
        .update(move_camera)
        .run();
}

fn setup(ctx: &mut Context) {
    ctx.spawn("camera").insert(Transform::default()).insert(Camera2d);

    // Persistent HUD — not managed by SceneManager, survives all transitions.
    ctx.create()
        .insert(Transform::from_xyz(-350.0, 250.0, 10.0))
        .insert(Sprite::new().color(Color::rgb(0.9, 0.9, 0.2)).size(15.0, 15.0))
        .tag("hud");
}

fn switch_scenes(ctx: &mut Context) {
    if ctx.input.just_pressed(KeyCode::Enter) {
        ctx.world.resource_mut::<SceneManager>().goto("game");
    }
    if ctx.input.just_pressed(KeyCode::Escape) {
        ctx.world.resource_mut::<SceneManager>().goto("menu");
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
