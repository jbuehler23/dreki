//! # Vertex — Per-Corner Data Sent to the GPU
//!
//! A *vertex* is a single point in a mesh. For 2D sprites, every quad has four
//! vertices (the corners). Each vertex carries data that the GPU needs to draw
//! it: a position, a texture coordinate (UV), and a tint color. These are
//! packed into a flat struct and uploaded to a GPU buffer each frame.
//!
//! ## Memory Layout
//!
//! The GPU reads vertex data as raw bytes at fixed offsets. `#[repr(C)]`
//! guarantees the Rust struct has the same layout a C compiler would produce —
//! no reordering, predictable padding. The `bytemuck` traits `Pod` (plain old
//! data) and `Zeroable` let us safely cast `&[SpriteVertex]` to `&[u8]` for
//! upload without any copies.
//!
//! ```text
//! SpriteVertex (36 bytes per vertex)
//! ┌────────────────┬──────────────┬────────────────────────┐
//! │ position       │ uv           │ color                  │
//! │ [f32; 3]       │ [f32; 2]     │ [f32; 4]               │
//! │ 12 bytes       │ 8 bytes      │ 16 bytes               │
//! │ offset 0       │ offset 12    │ offset 20              │
//! │ location(0)    │ location(1)  │ location(2)            │
//! └────────────────┴──────────────┴────────────────────────┘
//! ```
//!
//! The `shader_location` numbers tie each field to an `@location(N)` in the
//! WGSL shader. The GPU vertex fetcher uses `array_stride` (36) to step
//! between vertices and `offset` to find each attribute within a vertex.
//!
//! ## Why Position Is World-Space
//!
//! Positions are pre-transformed by the sprite's model matrix on the CPU. The
//! shader only applies the camera view-projection. This means sprites with
//! different positions/rotations/scales can share the same vertex buffer and
//! draw call as long as they use the same texture. The alternative — keeping
//! positions in local space and passing a per-sprite model matrix — would
//! require either instanced rendering or separate draw calls per sprite.
//!
//! ## Uniform Buffer (CameraUniform)
//!
//! A *uniform* is a small piece of data that stays constant across all vertices
//! in a draw call (unlike vertex attributes which change per-vertex). The
//! camera's view-projection matrix is a 4x4 float matrix (64 bytes) uploaded
//! once per frame. The vertex shader multiplies every vertex position by this
//! matrix to transform from world space into clip space (the [-1, 1] coordinate
//! system the GPU rasterizer expects).
//!
//! ## Comparison
//!
//! - **OpenGL**: Vertex attributes are configured with `glVertexAttribPointer`,
//!   specifying offset and stride manually. Same concept, more ceremony.
//! - **wgpu**: `VertexBufferLayout` describes the same thing declaratively.
//!   The `const` layout lets us define it at compile time.
//! - **Bevy**: Uses a proc-macro to auto-derive vertex layouts from struct
//!   fields. More ergonomic, but hides the byte-level details.

use bytemuck::{Pod, Zeroable};

/// Per-vertex data for sprite quads. Position is in world space (transformed
/// CPU-side), the shader only applies the camera view-projection.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct SpriteVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl SpriteVertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<SpriteVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            // position
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            // uv
            wgpu::VertexAttribute {
                offset: 12,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x2,
            },
            // color
            wgpu::VertexAttribute {
                offset: 20,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    };
}

/// Camera view-projection matrix uploaded as a uniform buffer.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}
