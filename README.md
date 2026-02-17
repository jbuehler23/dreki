# NECS

**N**ot-another **ECS** — a lightweight game framework for rapid prototyping, built with a custom ECS, wgpu rendering, and hot-reloadable assets.

## Features

- **Custom ECS** — archetype-based, zero unsafe, closure-driven queries
- **2D rendering** — sprites, text (fontdue), sprite-sheet animation, property tweening
- **3D rendering** — PBR (Cook-Torrance), glTF loading, built-in shapes, point/directional lights
- **Physics** — Rapier 2D/3D integration (opt-in)
- **Hot reload** — live texture and shader reloading via filesystem watcher
- **Diagnostics TUI** — real-time metrics dashboard (`necs-telemetry`)

## Quick Start

```rust
use necs::prelude::*;

fn main() {
    Game::new("My Game")
        .setup(setup)
        .run();
}

fn setup(ctx: &mut Context) {
    ctx.spawn("camera").insert(Transform::default()).insert(Camera2d);
    ctx.create()
        .insert(Transform::from_xy(0.0, 0.0))
        .insert(Sprite::new().color(Color::RED).size(100.0, 100.0));
}
```

## Running Examples

```sh
cargo run --example 2d
cargo run --example 3d
cargo run --example animation
cargo run --example text
cargo run --example audio
cargo run --example scene_hierarchy
cargo run --example scene_save_load
cargo run --example scene_switching
cargo run --example physics_2d --features physics2d
cargo run --example physics_3d --features physics3d
```

## License

MIT
