//! # Vertex — Per-Corner Data for 3D Meshes
//!
//! A 3D mesh vertex carries more data than a 2D sprite vertex: in addition to
//! position and texture coordinates, it needs a *surface normal* — the direction
//! the surface faces at that point. Normals are essential for lighting: the dot
//! product of the normal and the light direction tells you how bright the surface
//! should be.
//!
//! ## Memory Layout
//!
//! ```text
//! MeshVertex (32 bytes)
//! ┌──────────────┬──────────────┬──────────────┐
//! │ position     │ normal       │ uv           │
//! │ [f32; 3]     │ [f32; 3]     │ [f32; 2]     │
//! │ 12 bytes     │ 12 bytes     │ 8 bytes      │
//! │ offset 0     │ offset 12    │ offset 24    │
//! │ location(0)  │ location(1)  │ location(2)  │
//! └──────────────┴──────────────┴──────────────┘
//! ```
//!
//! 32 bytes is a clean power-of-two stride, which GPUs handle efficiently. We
//! omit tangent vectors (needed for normal mapping) to keep things simple —
//! that's a future phase.
//!
//! ## Uniform Buffers
//!
//! 3D rendering needs four categories of data beyond vertex attributes, each
//! changing at a different frequency:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │ Group 0 — Camera (per frame)                                │
//! │   view_proj: mat4x4  +  camera_pos: vec3  +  padding       │
//! │   80 bytes                                                  │
//! ├─────────────────────────────────────────────────────────────┤
//! │ Group 1 — Lights (per frame)                                │
//! │   1 directional light + ambient + 8 point lights + count    │
//! │   304 bytes                                                 │
//! ├─────────────────────────────────────────────────────────────┤
//! │ Group 2 — Material (per material)                           │
//! │   base_color, metallic, roughness, emissive                 │
//! │   48 bytes                                                  │
//! ├─────────────────────────────────────────────────────────────┤
//! │ Group 3 — Model (per object, dynamic offset)                │
//! │   model: mat4x4  +  normal_matrix: mat4x4                  │
//! │   128 bytes                                                 │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! Groups are ordered by change frequency. The GPU can keep groups 0-1 bound
//! all frame, only rebinding group 2 when the material changes and group 3
//! when the object changes. Group 3 uses *dynamic offsets* — one large buffer
//! holds all model matrices, and each draw call just passes a byte offset.
//!
//! ## Why `normal_matrix` Is mat4x4
//!
//! Mathematically, the normal matrix is 3x3 (the inverse transpose of the
//! upper-left 3x3 of the model matrix). But WGSL's uniform alignment rules
//! make mat3x3 awkward — each column is padded to 16 bytes anyway. Using
//! mat4x4 wastes 28 bytes but avoids alignment headaches. The shader simply
//! extracts the upper 3x3.
//!
//! ## Comparison
//!
//! - **Bevy**: Uses a flexible `MeshVertexBufferLayout` system that can
//!   include arbitrary attributes (tangents, colors, joints, weights).
//! - **wgpu examples**: Typically define vertex structs inline per demo.
//! - **glTF**: Defines a standard set of vertex attributes (POSITION, NORMAL,
//!   TEXCOORD_0, TANGENT, etc.). Our format matches the required subset.

use bytemuck::{Pod, Zeroable};

/// Per-vertex data for 3D meshes: position, surface normal, and texture UV.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl MeshVertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<MeshVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            // position: vec3<f32>
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            // normal: vec3<f32>
            wgpu::VertexAttribute {
                offset: 12,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x3,
            },
            // uv: vec2<f32>
            wgpu::VertexAttribute {
                offset: 24,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x2,
            },
        ],
    };
}

/// Camera uniform: view-projection matrix + world-space position.
///
/// The camera position is needed for specular reflection calculations — the
/// angle between the view direction and the reflected light direction determines
/// how bright the specular highlight is.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct CameraUniform3d {
    pub view_proj: [[f32; 4]; 4], // 64 bytes
    pub camera_pos: [f32; 3],     // 12 bytes
    pub _padding: f32,            // 4 bytes → total 80
}

/// Data for a single point light, packed for GPU upload.
///
/// 32 bytes per light: position (vec3 + pad), color (vec3 + pad) where the
/// padding slots hold intensity and radius.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct PointLightData {
    pub position: [f32; 3], // 12 bytes
    pub radius: f32,        // 4 bytes
    pub color: [f32; 3],    // 12 bytes
    pub intensity: f32,     // 4 bytes → total 32
}

/// Maximum number of point lights supported per frame. Fixed array avoids
/// dynamic shader arrays, which require more complex buffer management.
pub(crate) const MAX_POINT_LIGHTS: usize = 8;

/// Light uniform: all lighting data packed into one buffer.
///
/// Layout: directional light (32 bytes) + ambient (16 bytes) +
/// 8 point lights (256 bytes) + count (16 bytes with padding) = 320 bytes.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct LightUniform {
    // Directional light
    pub dir_direction: [f32; 3], // 12 bytes
    pub dir_intensity: f32,      // 4 bytes
    pub dir_color: [f32; 3],     // 12 bytes
    pub _pad0: f32,              // 4 bytes → 32

    // Ambient light
    pub ambient_color: [f32; 3], // 12 bytes
    pub ambient_intensity: f32,  // 4 bytes → 16

    // Point lights (fixed array of 8)
    pub point_lights: [PointLightData; MAX_POINT_LIGHTS], // 256 bytes

    // Count
    pub point_light_count: u32, // 4 bytes
    pub _pad1: [u32; 3],       // 12 bytes → 16
}

/// Material uniform: PBR metallic-roughness parameters.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct MaterialUniform {
    pub base_color: [f32; 4],  // 16 bytes
    pub metallic: f32,         // 4 bytes
    pub roughness: f32,        // 4 bytes
    pub _pad0: [f32; 2],       // 8 bytes → 32
    pub emissive: [f32; 3],    // 12 bytes
    pub _pad1: f32,            // 4 bytes → 48
}

/// Model uniform: transform + normal matrix, per object.
///
/// Uses dynamic offsets in the uniform buffer — one large buffer holds all
/// model matrices, and each draw call indexes into it with a byte offset.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ModelUniform {
    pub model: [[f32; 4]; 4],         // 64 bytes
    pub normal_matrix: [[f32; 4]; 4], // 64 bytes → total 128
}
