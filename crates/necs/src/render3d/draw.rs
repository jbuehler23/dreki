//! # Draw — The 3D Render Orchestrator
//!
//! This is the "main function" of the render3d subsystem. Each frame, the
//! engine calls [`render_meshes_3d`], which coordinates all the other modules
//! to produce a frame. Same role as `render_sprites_2d` in the 2D renderer.
//!
//! ## Per-Frame Flow
//!
//! ```text
//! render_meshes_3d(world)
//!   │
//!   ├─ 1. Lazy init ─── first frame only
//!   │     Create MeshRenderer, MeshStore, TextureStore3d
//!   │
//!   ├─ 2. Extract resources ─── remove from World
//!   │
//!   ├─ 3. Depth check ─── recreate depth texture if resized
//!   │
//!   ├─ 4. Collect lights ─── query lights → write LightUniform
//!   │
//!   ├─ 5. Camera VP ─── query Camera3d → perspective × inverse view
//!   │
//!   ├─ 6. Collect draw calls ─── query (Transform, Mesh3d, Material)
//!   │     Sort by material, write ModelUniforms to dynamic buffer
//!   │
//!   ├─ 7. Create material bind groups (group 2)
//!   │
//!   ├─ 8. Render pass
//!   │     Clear color+depth, bind pipeline
//!   │     Bind groups 0+1 once
//!   │     Loop: bind group 2 per material, group 3 per object
//!   │     draw_indexed for each object
//!   │
//!   └─ 9. Reinsert resources
//! ```
//!
//! ## Dynamic Model Buffer
//!
//! Group 3 uses a single large uniform buffer where each object's model
//! matrix is written at an aligned offset. The alignment is dictated by
//! `min_uniform_buffer_offset_alignment` from the GPU's limits (typically
//! 256 bytes). When binding group 3, we pass the byte offset for the
//! current object — this is the "dynamic offset" mechanism.
//!
//! ## Comparison
//!
//! - **Bevy**: Extraction happens in a separate "render world" with parallel
//!   systems. Multiple render phases (shadow, opaque, transparent) with
//!   sort keys and batching.
//! - **Our approach**: Single-pass forward rendering, serial extraction,
//!   minimal indirection.

use wgpu::util::DeviceExt;

use super::collect::{collect_camera, collect_draw_calls, collect_lights, DrawCall};
use super::mesh::MeshStore;
use super::pipeline::MeshRenderer;
use super::texture::{TextureHandle3d, TextureStore3d};
use super::vertex::MaterialUniform;
use crate::asset::{AssetKind, AssetServer};
use crate::ecs::World;
use crate::render::gpu::GpuContext;
use crate::render::pass::{ClearColor, FrameContext};

/// Render all 3D meshes for the current frame.
pub(crate) fn render_meshes_3d(world: &mut World, frame: &mut FrameContext<'_>) {
    let gpu = frame.gpu;

    // ── 1. Lazy init ────────────────────────────────────────────────────
    if !world.has_resource::<MeshRenderer>() {
        let renderer = MeshRenderer::new(gpu);
        let mesh_store = MeshStore::new(gpu);
        let texture_store = TextureStore3d::new(gpu);

        // Register shader file for hot-reload watching.
        let shader_path = renderer.shader_path.clone();
        world.insert_resource(renderer);
        world.insert_resource(mesh_store);
        world.insert_resource(texture_store);

        if let Some(path) = shader_path {
            if let Some(server) = world.get_resource_mut::<AssetServer>() {
                server.watch(path, AssetKind::Shader3d);
            }
        }
    }

    // ── 2. Extract resources ────────────────────────────────────────────
    let mut renderer = world
        .resource_remove::<MeshRenderer>()
        .expect("MeshRenderer missing");
    let mesh_store = world
        .resource_remove::<MeshStore>()
        .expect("MeshStore missing");
    let texture_store = world
        .resource_remove::<TextureStore3d>()
        .expect("TextureStore3d missing");

    // ── 3. Depth check ──────────────────────────────────────────────────
    let (sw, sh) = gpu.surface_size();
    renderer.resize_depth_if_needed(&gpu.device, sw, sh);

    // ── 4. Collect lights ───────────────────────────────────────────────
    let light_uniform = collect_lights(world);
    gpu.queue
        .write_buffer(&renderer.light_buffer, 0, bytemuck::cast_slice(&[light_uniform]));

    // ── 5. Camera ───────────────────────────────────────────────────────
    let camera_uniform = collect_camera(world, (sw, sh));
    gpu.queue
        .write_buffer(&renderer.camera_buffer, 0, bytemuck::cast_slice(&[camera_uniform]));

    // ── 6. Collect draw calls ───────────────────────────────────────────
    let draw_calls = collect_draw_calls(world);

    // Write model uniforms to the dynamic buffer
    let model_stride = if !draw_calls.is_empty() {
        let stride = renderer.ensure_model_capacity(&gpu.device, draw_calls.len());
        let mut model_data = vec![0u8; stride as usize * draw_calls.len()];
        for (i, call) in draw_calls.iter().enumerate() {
            let offset = i * stride as usize;
            let bytes = bytemuck::bytes_of(&call.model_uniform);
            model_data[offset..offset + bytes.len()].copy_from_slice(bytes);
        }
        gpu.queue.write_buffer(&renderer.model_buffer, 0, &model_data);
        stride
    } else {
        0
    };

    // ── 7. Create material bind groups ──────────────────────────────────
    let material_bind_groups = create_material_bind_groups(
        gpu,
        &renderer,
        &texture_store,
        &draw_calls,
    );

    // ── 8. Render pass ──────────────────────────────────────────────────
    let clear_color = world
        .get_resource::<ClearColor>()
        .copied()
        .unwrap_or_default();

    {
        let mut render_pass = frame.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("3d render pass"),
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
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &renderer.depth_texture,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        if !draw_calls.is_empty() {
            render_pass.set_pipeline(&renderer.pipeline);
            render_pass.set_bind_group(0, &renderer.camera_bind_group, &[]);
            render_pass.set_bind_group(1, &renderer.light_bind_group, &[]);

            let mut current_material_idx: Option<usize> = None;

            for (i, call) in draw_calls.iter().enumerate() {
                // Bind material group 2 only when it changes
                let mat_idx = material_bind_groups
                    .iter()
                    .position(|m| m.draw_indices.contains(&i))
                    .unwrap_or(0);

                if current_material_idx != Some(mat_idx) {
                    render_pass.set_bind_group(
                        2,
                        &material_bind_groups[mat_idx].bind_group,
                        &[],
                    );
                    current_material_idx = Some(mat_idx);
                }

                // Bind model group 3 with dynamic offset
                let dynamic_offset = i as u32 * model_stride;
                render_pass.set_bind_group(3, &renderer.model_bind_group, &[dynamic_offset]);

                // Bind mesh buffers and draw
                let gpu_mesh = mesh_store.get(call.mesh);
                render_pass.set_vertex_buffer(0, gpu_mesh.vertex_buffer.slice(..));
                render_pass.set_index_buffer(gpu_mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..gpu_mesh.index_count, 0, 0..1);
            }
        }
    }

    // ── 8b. Debug wireframes ────────────────────────────────────────────
    #[cfg(feature = "physics3d")]
    {
        use super::debug_wireframe::{DebugColliders3d, DebugWireframeRenderer, render_debug_wireframes_3d};
        use crate::physics3d::Collider3d;

        if world.has_resource::<DebugColliders3d>() {
            // Lazy-init the debug renderer
            if !world.has_resource::<DebugWireframeRenderer>() {
                let dbg_renderer = DebugWireframeRenderer::new(
                    &gpu.device,
                    gpu.surface_format(),
                    &renderer.camera_bind_group_layout,
                );
                world.insert_resource(dbg_renderer);
            }

            // Collect collider poses from ECS components directly
            let mut poses = Vec::new();
            world.query::<(&Collider3d, &crate::math::Transform)>(|_entity, (coll, tf)| {
                poses.push((tf.translation, tf.rotation, coll.shape));
            });

            if let Some(mut dbg_renderer) = world.resource_remove::<DebugWireframeRenderer>() {
                if let Some(debug_config) = world.resource_remove::<DebugColliders3d>() {
                    render_debug_wireframes_3d(
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
        stats.draw_calls = draw_calls.len() as u32;
        stats.vertices = draw_calls.iter().map(|c| mesh_store.get(c.mesh).index_count).sum();
        stats.textures_loaded = texture_store.entries.len() as u32;
    }

    // ── 9. Reinsert resources ───────────────────────────────────────────
    world.insert_resource(renderer);
    world.insert_resource(mesh_store);
    world.insert_resource(texture_store);
}

/// A material bind group (group 2) shared by one or more draw calls.
struct MaterialBindGroupEntry {
    bind_group: wgpu::BindGroup,
    draw_indices: Vec<usize>,
}

/// Create material bind groups, deduplicating when consecutive draw calls
/// share the same material parameters and texture.
fn create_material_bind_groups(
    gpu: &GpuContext,
    renderer: &MeshRenderer,
    texture_store: &TextureStore3d,
    draw_calls: &[DrawCall],
) -> Vec<MaterialBindGroupEntry> {
    if draw_calls.is_empty() {
        return Vec::new();
    }

    let mut groups: Vec<MaterialBindGroupEntry> = Vec::new();

    for (i, call) in draw_calls.iter().enumerate() {
        // Check if this call matches the current material group
        let matches_last = groups.last().map_or(false, |last| {
            let last_idx = last.draw_indices[0];
            same_material(
                &draw_calls[last_idx].material_uniform,
                draw_calls[last_idx].base_color_texture,
                &call.material_uniform,
                call.base_color_texture,
            )
        });

        if matches_last {
            groups.last_mut().unwrap().draw_indices.push(i);
        } else {
            // Create a new material bind group
            let mat_buffer =
                gpu.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("3d material buffer"),
                        contents: bytemuck::cast_slice(&[call.material_uniform]),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });

            let tex_handle = call
                .base_color_texture
                .unwrap_or(texture_store.default_handle());
            let tex_entry = texture_store.get(tex_handle);

            let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("3d material bind group"),
                layout: &renderer.material_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: mat_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&tex_entry.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&renderer.sampler),
                    },
                ],
            });

            groups.push(MaterialBindGroupEntry {
                bind_group,
                draw_indices: vec![i],
            });
        }
    }

    groups
}

/// Check if two materials are identical (same uniform data and texture).
fn same_material(
    a_uniform: &MaterialUniform,
    a_tex: Option<TextureHandle3d>,
    b_uniform: &MaterialUniform,
    b_tex: Option<TextureHandle3d>,
) -> bool {
    a_tex == b_tex
        && a_uniform.base_color == b_uniform.base_color
        && a_uniform.metallic == b_uniform.metallic
        && a_uniform.roughness == b_uniform.roughness
        && a_uniform.emissive == b_uniform.emissive
}
