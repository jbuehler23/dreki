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

    App::new()
        .add_plugins(DefaultPlugins)
        .set_title("dreki — hot reload test")
        .insert_resource(ClearColor([0.12, 0.12, 0.18, 1.0]))
        .insert_resource(TexturePath(test_png))
        .add_startup_system(setup)
        .run();
}

struct TexturePath(PathBuf);

fn setup(world: &mut World) {
    world.spawn((Transform::default(), Camera2d));

    let path = world.resource::<TexturePath>().0.to_string_lossy().to_string();
    let tex = load_texture(world, &path);

    world.spawn((
        Transform::default(),
        Sprite {
            texture: Some(tex),
            ..Default::default()
        },
    ));
}
