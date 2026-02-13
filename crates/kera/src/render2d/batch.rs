//! # Batch — Collect, Sort, and Group 2D Primitives for Drawing
//!
//! This module is the CPU-side heart of the 2D renderer. Each frame it:
//! 1. Queries all `(Transform, Sprite)`, `(Transform, Shape2d)`, and text entities
//! 2. Emits vertices and indices per primitive (quads for sprites/text, tessellated
//!    geometry for shapes)
//! 3. Sorts by Z for correct back-to-front ordering
//! 4. Groups consecutive same-texture primitives into batches
//!
//! ## Why Batching Matters
//!
//! Every `draw_indexed` call carries CPU overhead: the driver validates state,
//! the GPU may stall between draws. A scene with 500 sprites and 500 draw
//! calls is much slower than 500 sprites in 3 draw calls (one per texture).
//! Batching converts the former into the latter by merging sprites that share
//! the same texture into a single contiguous range of indices.
//!
//! ## Primitives
//!
//! Sprites and text glyphs emit quads (4 vertices, 6 indices). Shapes emit
//! variable-count geometry from CPU tessellation — circles have a center
//! + rim vertices, rectangles are quads, etc. All use the same vertex format
//! and are mixed freely in the Z-sorted draw order.
//!
//! ## Painter's Algorithm
//!
//! After collecting all primitives, they're sorted by `Transform.translation.z`
//! in ascending order (lowest Z = farthest away). Drawing them in this order
//! means closer primitives paint over farther ones, which gives correct layering
//! with alpha blending.
//!
//! ## Texture Batching
//!
//! After sorting, primitives are iterated in order. As long as consecutive
//! primitives share the same texture handle, they're merged into one
//! [`DrawBatch`]. Shapes always use texture handle 0 (the 1x1 white texture),
//! so they batch with untextured sprites.
//!
//! ## Comparison
//!
//! - **Bevy**: Uses a `SpriteBatch` system that sorts by Z and texture,
//!   produces instanced draw calls with a per-instance transform buffer.
//!   More GPU-friendly at scale, but far more code.
//! - **Macroquad**: Builds vertex buffers CPU-side each frame with immediate
//!   mode calls. No explicit batching — each draw is its own call.
//! - **Love2D** (C++ backend): Automatic batching of consecutive same-texture
//!   draws, very similar to our approach.

use crate::ecs::World;
use crate::math::Transform;

use super::font::FontStore;
use super::shapes::Shape2d;
use super::texture::{TextureHandle, TextureStore};
use super::vertex::SpriteVertex;
use super::{Camera2d, Sprite};
use super::font::Text;

/// A draw command for one batch of primitives sharing the same texture.
pub(crate) struct DrawBatch {
    pub texture: TextureHandle,
    /// Range into the shared index buffer.
    pub index_start: u32,
    pub index_count: u32,
}

/// Intermediate primitive data collected from the ECS before sorting.
struct CollectedPrimitive {
    z: f32,
    texture: TextureHandle,
    vertices: Vec<SpriteVertex>,
    /// Local indices (0-based) into `vertices`.
    indices: Vec<u32>,
}

/// Collect all sprites, shapes, and text, emit geometry, sort by Z, batch by texture.
///
/// `surface_size` is passed in because `GpuContext` has been extracted from the
/// world by the caller.
///
/// Returns (vertices, indices, batches, camera_view_proj).
pub(crate) fn collect_and_batch(
    world: &mut World,
    texture_store: &TextureStore,
    font_store: Option<&FontStore>,
    surface_size: (u32, u32),
) -> (Vec<SpriteVertex>, Vec<u32>, Vec<DrawBatch>, glam::Mat4) {
    // Camera view-projection
    let view_proj = compute_camera_vp(world, surface_size);

    // Collect sprites
    let default_handle = texture_store.default_handle();
    let mut collected: Vec<CollectedPrimitive> = Vec::new();

    world.query::<(&Transform, &Sprite)>(|_entity, (transform, sprite)| {
        let tex_handle = sprite.texture.unwrap_or(default_handle);

        // Determine sprite size
        let size = if sprite.size != glam::Vec2::ZERO {
            sprite.size
        } else if let Some(tex_handle_inner) = sprite.texture {
            let entry = texture_store.get(tex_handle_inner);
            glam::Vec2::new(entry.width as f32, entry.height as f32)
        } else {
            // No texture, no explicit size — default to 64x64
            glam::Vec2::new(64.0, 64.0)
        };

        let half = size * 0.5;
        let color = sprite.color.to_array();

        // UV coordinates from texture_rect (with flip support)
        let rect = &sprite.texture_rect;
        let (u_min, u_max) = if sprite.flip_x {
            (rect.max.x, rect.min.x)
        } else {
            (rect.min.x, rect.max.x)
        };
        let (v_min, v_max) = if sprite.flip_y {
            (rect.max.y, rect.min.y)
        } else {
            (rect.min.y, rect.max.y)
        };

        // Quad corners in local space, then transformed by model matrix
        let model = transform.matrix();
        let corners = [
            glam::Vec3::new(-half.x, -half.y, 0.0), // bottom-left
            glam::Vec3::new( half.x, -half.y, 0.0), // bottom-right
            glam::Vec3::new( half.x,  half.y, 0.0), // top-right
            glam::Vec3::new(-half.x,  half.y, 0.0), // top-left
        ];
        let uvs = [
            [u_min, v_max], // bottom-left
            [u_max, v_max], // bottom-right
            [u_max, v_min], // top-right
            [u_min, v_min], // top-left
        ];

        let mut vertices = Vec::with_capacity(4);
        for i in 0..4 {
            let world_pos = model.transform_point3(corners[i]);
            vertices.push(SpriteVertex {
                position: [world_pos.x, world_pos.y, world_pos.z],
                uv: uvs[i],
                color,
            });
        }

        collected.push(CollectedPrimitive {
            z: transform.translation.z,
            texture: tex_handle,
            vertices,
            indices: vec![0, 1, 2, 0, 2, 3],
        });
    });

    // Collect Shape2d entities
    world.query::<(&Transform, &Shape2d)>(|_entity, (transform, shape)| {
        let (positions, local_indices) = shape.tessellate();
        let model = transform.matrix();
        let color = shape.color.to_array();

        let vertices: Vec<SpriteVertex> = positions
            .iter()
            .map(|pos| {
                let world_pos = model.transform_point3(glam::Vec3::new(pos[0], pos[1], 0.0));
                SpriteVertex {
                    position: [world_pos.x, world_pos.y, world_pos.z],
                    uv: [0.5, 0.5], // center of white texture
                    color,
                }
            })
            .collect();

        collected.push(CollectedPrimitive {
            z: transform.translation.z,
            texture: default_handle,
            vertices,
            indices: local_indices,
        });
    });

    // Collect text entities as glyph quads
    if let Some(fs) = font_store {
        world.query::<(&Transform, &Text)>(|_entity, (transform, text)| {
            let entry = fs.get(text.font);
            let color = text.color.to_array();
            let z = transform.translation.z;
            let model = transform.matrix();

            let mut cursor_x: f32 = 0.0;
            let mut cursor_y: f32 = 0.0;

            for ch in text.content.chars() {
                if ch == '\n' {
                    cursor_x = 0.0;
                    cursor_y -= entry.line_height;
                    continue;
                }

                let glyph = match entry.glyph(ch) {
                    Some(g) => g,
                    None => continue,
                };

                // Skip zero-size glyphs (e.g. space) — just advance cursor
                if glyph.width == 0.0 || glyph.height == 0.0 {
                    cursor_x += glyph.advance;
                    continue;
                }

                // Glyph quad in local space relative to Transform origin.
                // offset_y is ymin from fontdue (baseline-relative, Y-up).
                let x0 = cursor_x + glyph.offset_x;
                let y0 = cursor_y + glyph.offset_y;
                let x1 = x0 + glyph.width;
                let y1 = y0 + glyph.height;

                let corners = [
                    glam::Vec3::new(x0, y0, 0.0), // bottom-left
                    glam::Vec3::new(x1, y0, 0.0), // bottom-right
                    glam::Vec3::new(x1, y1, 0.0), // top-right
                    glam::Vec3::new(x0, y1, 0.0), // top-left
                ];
                let uvs = [
                    [glyph.u_min, glyph.v_max], // bottom-left
                    [glyph.u_max, glyph.v_max], // bottom-right
                    [glyph.u_max, glyph.v_min], // top-right
                    [glyph.u_min, glyph.v_min], // top-left
                ];

                let mut vertices = Vec::with_capacity(4);
                for i in 0..4 {
                    let world_pos = model.transform_point3(corners[i]);
                    vertices.push(SpriteVertex {
                        position: [world_pos.x, world_pos.y, world_pos.z],
                        uv: uvs[i],
                        color,
                    });
                }

                collected.push(CollectedPrimitive {
                    z,
                    texture: entry.atlas_handle,
                    vertices,
                    indices: vec![0, 1, 2, 0, 2, 3],
                });

                cursor_x += glyph.advance;
            }
        });
    }

    // Sort by Z ascending (back-to-front for painter's algorithm)
    collected.sort_by(|a, b| a.z.partial_cmp(&b.z).unwrap_or(std::cmp::Ordering::Equal));

    // Emit vertices, indices, and batches
    let mut vertices = Vec::with_capacity(collected.len() * 4);
    let mut indices = Vec::with_capacity(collected.len() * 6);
    let mut batches: Vec<DrawBatch> = Vec::new();

    for prim in &collected {
        let base_vertex = vertices.len() as u32;

        vertices.extend_from_slice(&prim.vertices);

        // Offset local indices by base_vertex
        let idx_start = indices.len();
        for &local_idx in &prim.indices {
            indices.push(base_vertex + local_idx);
        }
        let idx_count = (indices.len() - idx_start) as u32;

        // Extend current batch or start a new one
        if let Some(last) = batches.last_mut() {
            if last.texture == prim.texture {
                last.index_count += idx_count;
                continue;
            }
        }
        batches.push(DrawBatch {
            texture: prim.texture,
            index_start: idx_start as u32,
            index_count: idx_count,
        });
    }

    (vertices, indices, batches, view_proj)
}

/// Compute the camera view-projection matrix from the Camera2d entity.
fn compute_camera_vp(world: &mut World, surface_size: (u32, u32)) -> glam::Mat4 {
    let (width, height) = surface_size;
    let half_w = width as f32 / 2.0;
    let half_h = height as f32 / 2.0;

    // Orthographic projection: Y-up, origin at center
    let projection = glam::Mat4::orthographic_rh(-half_w, half_w, -half_h, half_h, -1000.0, 1000.0);

    // Camera transform (inverse = view matrix)
    let mut camera_transform = Transform::IDENTITY;
    world.query_single::<(&Transform,), Camera2d>(|_entity, (transform,)| {
        camera_transform = *transform;
    });

    let view = camera_transform.matrix().inverse();
    projection * view
}
