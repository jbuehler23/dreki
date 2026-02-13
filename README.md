# Kera

A lightweight game framework for rapid prototyping, built with a custom ECS, wgpu rendering, and hot-reloadable assets.

## Features

- **Custom ECS** — archetype-based, zero unsafe, closure-driven queries
- **2D rendering** — sprites, text (fontdue), sprite-sheet animation, property tweening
- **3D rendering** — PBR (Cook-Torrance), glTF loading, built-in shapes, point/directional lights
- **Physics** — Rapier 2D/3D integration (opt-in)
- **Hot reload** — live texture and shader reloading via filesystem watcher
- **Diagnostics TUI** — real-time metrics dashboard (`kera-telemetry`)

## Quick Start

```rust
use kera::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_startup_system(setup)
        .run();
}

fn setup(world: &mut World) {
    world.spawn((Transform::default(), Camera2d));
    world.spawn((
        Transform::from_xy(0.0, 0.0),
        Sprite {
            color: Color::RED,
            size: Vec2::new(100.0, 100.0),
            ..Default::default()
        },
    ));
}
```

## Running Examples

```sh
cargo run --example 2d
cargo run --example 3d
cargo run --example animation
cargo run --example text
cargo run --example physics_2d --features physics2d
cargo run --example physics_3d --features physics3d
```

## License

MIT
