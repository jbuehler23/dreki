//! Hot-reload test example.
//!
//! Displays `examples/assets/test.png` as a sprite. While running, overwrite
//! the PNG with a different image — the sprite updates live.

use std::path::PathBuf;

use dreki::prelude::*;

fn main() {
    env_logger::init();

    let test_png = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets")
        .join("test.png");

    Game::new("dreki — hot reload test")
        .resource(ClearColor([0.12, 0.12, 0.18, 1.0]))
        .resource(TexturePath(test_png))
        .setup(setup)
        .run();
}

struct TexturePath(PathBuf);

fn setup(ctx: &mut Context) {
    ctx.spawn("camera").insert(Transform::default()).insert(Camera2d);

    let path = ctx.world.resource::<TexturePath>().0.to_string_lossy().to_string();
    let tex = load_texture(&mut ctx.world, &path);

    ctx.create()
        .insert(Transform::default())
        .insert(Sprite::new().texture(tex));
}
