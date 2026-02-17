//! # Draw — The Orchestrator
//!
//! This is the "main function" of the render2d subsystem. Each frame, the
//! engine calls [`render_sprites_2d`], which coordinates all the other modules
//! to produce a frame. It is the only public entry point from the rest of the
//! engine into the 2D renderer.
//!
//! ## Per-Frame Flow
//!
//! ```text
//! render_sprites_2d(world)
//!   │
//!   ├─ 1. Lazy init ─── first frame only
//!   │     Create SpriteRenderer (pipeline, camera buffer, sampler)
//!   │     Create TextureStore (with 1x1 white default)
//!   │     Insert both as resources into World
//!   │
//!   ├─ 2. Extract resources ─── remove GpuContext, SpriteRenderer,
//!   │     TextureStore from World (we need &mut and & simultaneously)
//!   │
//!   ├─ 3. Collect & batch ─── calls batch::collect_and_batch()
//!   │     Query sprites, emit quads, Z-sort, group by texture
//!   │     Returns (vertices, indices, batches, view_proj)
//!   │
//!   ├─ 4. Upload to GPU
//!   │     Write camera uniform to buffer
//!   │     Create fresh vertex + index buffers with frame's data
//!   │
//!   ├─ 5. Render pass
//!   │     Acquire surface texture
//!   │     Clear with ClearColor
//!   │     Bind pipeline + camera
//!   │     For each batch: bind texture, draw_indexed(range)
//!   │     Submit command buffer, present
//!   │
//!   └─ 6. Reinsert resources ─── put GpuContext, SpriteRenderer,
//!         TextureStore back into World
//! ```
//!
//! ## The Extract/Reinsert Dance
//!
//! The core challenge is Rust's borrow rules: we need mutable access to the
//! `SpriteRenderer` (to update its buffers), shared access to `GpuContext`
//! (for the device and queue), *and* mutable access to `World` (to query
//! entities). All three live inside `World` as resources.
//!
//! The solution is to temporarily *remove* resources from the world with
//! `resource_remove()`, which returns owned values. With the resources out,
//! the world is free to be queried. After the frame, everything is reinserted.
//! This is safe, simple, and avoids `RefCell`/`Mutex` overhead — the only cost
//! is three HashMap removes and inserts per frame.
//!
//! ## Surface Errors
//!
//! `gpu.surface.get_current_texture()` can fail with:
//! - `SurfaceError::Timeout` — GPU is busy; the caller can retry next frame
//! - `SurfaceError::Outdated` — surface needs reconfiguring (e.g., after
//!   resize); the engine's resize handler reconfigures, then next frame works
//! - `SurfaceError::Lost` — GPU device lost; recovery requires recreating
//!   the surface (not yet implemented; would require full reinitialization)
//! - `SurfaceError::OutOfMemory` — fatal; nothing to do but exit
//!
//! We propagate the error to the caller (`render_frame` in `render/pass.rs`),
//! which handles `Outdated` by reconfiguring and retrying.

use wgpu::util::DeviceExt;

use super::batch::collect_and_batch;
use super::font::FontStore;
use super::pipeline::SpriteRenderer;
use super::texture::TextureStore;
use super::vertex::CameraUniform;
use crate::asset::{AssetKind, AssetServer};
use crate::ecs::World;
use crate::render::pass::{ClearColor, FrameContext};

/// Render all 2D sprites for the current frame.
///
/// This is called from `render_frame` in `render/pass.rs`. It handles:
/// 1. Lazy initialization of SpriteRenderer + TextureStore
/// 2. Camera uniform update
/// 3. Sprite collection, sorting, batching
/// 4. GPU buffer upload
/// 5. Render pass with draw calls
pub(crate) fn render_sprites_2d(world: &mut World, frame: &mut FrameContext<'_>) {
    let gpu = frame.gpu;

    // Lazy init: create SpriteRenderer and TextureStore on first call
    if !world.has_resource::<SpriteRenderer>() {
        let renderer = SpriteRenderer::new(gpu);
        let texture_store = TextureStore::new(gpu, &renderer);

        // Register shader file for hot-reload watching.
        let shader_path = renderer.shader_path.clone();
        world.insert_resource(renderer);
        world.insert_resource(texture_store);

        if let Some(path) = shader_path {
            if let Some(server) = world.get_resource_mut::<AssetServer>() {
                server.watch(path, AssetKind::Shader2d);
            }
        }
    }

    // Extract resources to avoid borrow conflicts
    let mut renderer = world
        .resource_remove::<SpriteRenderer>()
        .expect("SpriteRenderer missing");
    let texture_store = world
        .resource_remove::<TextureStore>()
        .expect("TextureStore missing");
    let font_store = world.resource_remove::<FontStore>();

    // Collect and batch sprites + text (world is free to query now)
    let surface_size = gpu.surface_size();
    let (vertices, indices, batches, view_proj) =
        collect_and_batch(world, &texture_store, font_store.as_ref(), surface_size);

    // Update camera uniform
    let camera_uniform = CameraUniform {
        view_proj: view_proj.to_cols_array_2d(),
    };
    gpu.queue
        .write_buffer(&renderer.camera_buffer, 0, bytemuck::cast_slice(&[camera_uniform]));

    // Upload vertex and index buffers
    if !vertices.is_empty() {
        let vertex_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sprite vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sprite index buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        renderer.vertex_buffer = Some(vertex_buffer);
        renderer.index_buffer = Some(index_buffer);
    } else {
        renderer.vertex_buffer = None;
        renderer.index_buffer = None;
    }

    // Get clear color
    let clear_color = world
        .get_resource::<ClearColor>()
        .copied()
        .unwrap_or_default();

    {
        let mut render_pass = frame.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sprite render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &frame.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: clear_color.0[0],
                        g: clear_color.0[1],
                        b: clear_color.0[2],
                        a: clear_color.0[3],
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        if let (Some(vb), Some(ib)) = (&renderer.vertex_buffer, &renderer.index_buffer) {
            render_pass.set_pipeline(&renderer.pipeline);
            render_pass.set_bind_group(0, &renderer.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, vb.slice(..));
            render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);

            for batch in &batches {
                let entry = texture_store.get(batch.texture);
                render_pass.set_bind_group(1, &entry.bind_group, &[]);
                render_pass.draw_indexed(
                    batch.index_start..(batch.index_start + batch.index_count),
                    0,
                    0..1,
                );
            }
        }
    }

    // ── Debug wireframes ──────────────────────────────────────────────
    #[cfg(feature = "physics2d")]
    {
        use super::debug_wireframe::{DebugColliders2d, DebugWireframeRenderer2d, render_debug_wireframes_2d};
        use crate::physics2d::Collider2d;

        if world.has_resource::<DebugColliders2d>() {
            // Lazy-init the debug renderer
            if !world.has_resource::<DebugWireframeRenderer2d>() {
                let dbg_renderer = DebugWireframeRenderer2d::new(
                    &gpu.device,
                    gpu.surface_format(),
                    &renderer.camera_bind_group_layout,
                );
                world.insert_resource(dbg_renderer);
            }

            // Collect collider poses from ECS components directly
            let mut poses = Vec::new();
            world.query::<(&Collider2d, &crate::math::Transform)>(|_entity, (coll, tf)| {
                let angle = {
                    let (z, _y, _x) = tf.rotation.to_euler(glam::EulerRot::ZYX);
                    z
                };
                poses.push((glam::Vec2::new(tf.translation.x, tf.translation.y), angle, coll.shape));
            });

            if let Some(mut dbg_renderer) = world.resource_remove::<DebugWireframeRenderer2d>() {
                if let Some(debug_config) = world.resource_remove::<DebugColliders2d>() {
                    render_debug_wireframes_2d(
                        &mut frame.encoder,
                        &frame.view,
                        gpu,
                        &renderer,
                        &mut dbg_renderer,
                        &debug_config,
                        &poses,
                    );
                    world.insert_resource(debug_config);
                }
                world.insert_resource(dbg_renderer);
            }
        }
    }

    // Update diagnostics render stats.
    #[cfg(feature = "diagnostics")]
    if let Some(stats) = world.get_resource_mut::<crate::diag::RenderStats>() {
        stats.draw_calls = batches.len() as u32;
        stats.vertices = vertices.len() as u32;
        stats.textures_loaded = texture_store.entries.len() as u32;
    }

    // Reinsert resources
    world.insert_resource(renderer);
    world.insert_resource(texture_store);
    if let Some(fs) = font_store {
        world.insert_resource(fs);
    }
}
