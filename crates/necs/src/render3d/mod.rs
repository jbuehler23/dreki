//! # Render3d — 3D PBR Mesh Rendering
//!
//! A physically-based 3D renderer that draws textured meshes with metallic-
//! roughness materials, directional and point lights, and a perspective camera.
//! Built on the same patterns as the 2D sprite renderer: lazy initialization,
//! extract/reinsert for borrow safety, and handle-based resource management.
//!
//! ## Architecture
//!
//! ```text
//!  Camera3d + Transform              (Mesh3d + Material + Transform) × N
//!         │                                      │
//!         ▼                                      ▼
//!   ┌───────────────┐          ┌──────────────────────────────────┐
//!   │ perspective   │          │       collect draw calls          │
//!   │ projection ×  │          │  query entities, build model      │
//!   │ inverse view  │          │  + normal matrices, sort by       │
//!   └───────┬───────┘          │  material to minimize rebinds    │
//!           │                  └──────────────┬───────────────────┘
//!           │                                 │
//!           │      DirectionalLight           │
//!           │      PointLight × N             │
//!           │      AmbientLight               │
//!           │           │                     │
//!           ▼           ▼                     ▼
//!   ┌─────────────────────────────────────────────────────┐
//!   │  GPU render pass                                     │
//!   │  • bind groups 0+1 once (camera + lights)           │
//!   │  • for each material: bind group 2 (material+tex)   │
//!   │  • for each object: bind group 3 (model, dyn offset)│
//!   │  • bind mesh buffers, draw_indexed                   │
//!   │  • depth buffer for correct occlusion                │
//!   └─────────────────────────────────────────────────────┘
//! ```
//!
//! ## PBR (Physically Based Rendering)
//!
//! PBR materials define surfaces using physical properties rather than ad-hoc
//! color tweaks. The metallic-roughness model splits surfaces into:
//!
//! - **Metallic** (0.0–1.0): How much the surface behaves like metal. Metals
//!   reflect their base color; non-metals (dielectrics) reflect white.
//! - **Roughness** (0.0–1.0): How rough the surface is. Rough surfaces scatter
//!   light diffusely; smooth surfaces produce sharp specular highlights.
//!
//! The shader implements a simplified Cook-Torrance BRDF — the same model used
//! by Unreal Engine, Unity, Blender, and glTF. See `shader.wgsl` for the full
//! breakdown with educational comments.
//!
//! ## Bind Group Strategy
//!
//! Four bind groups ordered by change frequency:
//!
//! | Group | Content | Changes | Strategy |
//! |-------|---------|---------|----------|
//! | 0 | Camera VP + position | Once/frame | Single uniform buffer |
//! | 1 | All lights | Once/frame | Single uniform buffer |
//! | 2 | Material params + texture | Per material | Recreated per frame |
//! | 3 | Model + normal matrices | Per object | Dynamic uniform buffer |
//!
//! Group 3 uses *dynamic offsets*: one large buffer holds all model matrices
//! at aligned offsets. Each draw call passes a different offset, avoiding
//! per-object bind group creation.
//!
//! ## Comparison
//!
//! - **Bevy**: Full PBR with cascaded shadow maps, SSAO, bloom, tone mapping,
//!   clustered forward rendering for hundreds of lights. Far more complex.
//! - **three.js**: `MeshStandardMaterial` implements the same PBR model.
//!   WebGL/WebGPU backend handles bind groups automatically.
//! - **Our approach**: Minimal forward renderer with fixed point light limit
//!   (8) and no shadows. Optimized for clarity and learning.

pub(crate) mod collect;
pub(crate) mod draw;
pub(crate) mod mesh;
pub(crate) mod pipeline;
pub mod shape;
pub(crate) mod shapes;
pub(crate) mod texture;
pub(crate) mod vertex;

pub(crate) mod gltf;
#[cfg(feature = "physics3d")]
pub(crate) mod debug_wireframe;

#[cfg(feature = "physics3d")]
pub use debug_wireframe::DebugColliders3d;
pub use mesh::MeshHandle;
pub use shape::{Shape3d, ShapeKind3d};
pub use texture::{TextureHandle3d, load_texture_3d};
pub use self::gltf::load_gltf;

use crate::math::Vec3;
use mesh::{mesh_cube, mesh_cylinder, mesh_plane, mesh_sphere};

/// Marker component for a 3D perspective camera. Pair with
/// [`Transform`](crate::math::Transform).
///
/// The camera uses a perspective projection: objects farther away appear
/// smaller, just like in real life. Field of view, near plane, and far plane
/// control the visible volume (the *frustum*).
#[derive(Debug)]
pub struct Camera3d {
    /// Vertical field of view in degrees. Default: 45.
    pub fov_y: f32,
    /// Near clipping plane distance. Objects closer than this are invisible.
    pub near: f32,
    /// Far clipping plane distance. Objects farther than this are invisible.
    pub far: f32,
}

impl Default for Camera3d {
    fn default() -> Self {
        Self {
            fov_y: 45.0,
            near: 0.1,
            far: 1000.0,
        }
    }
}

/// A 3D mesh component. References a mesh in the [`MeshStore`](mesh::MeshStore)
/// via a [`MeshHandle`].
///
/// Pair with [`Transform`](crate::math::Transform) and [`Material`] to render.
#[derive(Debug)]
pub struct Mesh3d {
    pub mesh: MeshHandle,
}

impl Mesh3d {
    /// Create a [`Mesh3d`] referencing the built-in cube.
    pub fn cube() -> Self {
        Self { mesh: mesh_cube() }
    }

    /// Create a [`Mesh3d`] referencing the built-in plane.
    pub fn plane() -> Self {
        Self { mesh: mesh_plane() }
    }

    /// Create a [`Mesh3d`] referencing the built-in sphere.
    pub fn sphere() -> Self {
        Self { mesh: mesh_sphere() }
    }

    /// Create a [`Mesh3d`] referencing the built-in cylinder.
    pub fn cylinder() -> Self {
        Self { mesh: mesh_cylinder() }
    }
}

/// PBR metallic-roughness material.
///
/// Controls how a surface interacts with light. Inspired by the glTF 2.0
/// material model, which is the industry standard for real-time PBR.
///
/// ## Quick Guide
///
/// | Surface | metallic | roughness | base_color |
/// |---------|----------|-----------|------------|
/// | Plastic | 0.0 | 0.5 | any color |
/// | Rubber | 0.0 | 0.9 | dark color |
/// | Gold | 1.0 | 0.3 | (1.0, 0.766, 0.336) |
/// | Mirror | 1.0 | 0.0 | (0.95, 0.95, 0.95) |
/// | Rough metal | 1.0 | 0.8 | any metallic color |
#[derive(Debug)]
pub struct Material {
    /// Base color (albedo). Alpha channel is ignored (opaque only).
    pub base_color: [f32; 4],
    /// Optional base color texture. Sampled and multiplied with `base_color`.
    pub base_color_texture: Option<TextureHandle3d>,
    /// Metallic factor [0.0, 1.0]. 0 = dielectric, 1 = metal.
    pub metallic: f32,
    /// Roughness factor [0.0, 1.0]. 0 = mirror-smooth, 1 = fully rough.
    pub roughness: f32,
    /// Emissive color (self-illumination), added after lighting.
    pub emissive: [f32; 3],
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            base_color_texture: None,
            metallic: 0.0,
            roughness: 0.5,
            emissive: [0.0, 0.0, 0.0],
        }
    }
}

/// A directional light (like the sun). No position — only direction.
///
/// Directional lights emit parallel rays from infinitely far away. Every
/// surface in the scene receives the same light direction regardless of
/// position. Only one is supported per scene (the first found is used).
#[derive(Debug)]
pub struct DirectionalLight {
    /// Direction the light is shining *toward* (normalized in shader).
    pub direction: Vec3,
    /// Light color (linear RGB).
    pub color: [f32; 3],
    /// Intensity multiplier.
    pub intensity: f32,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            direction: Vec3::new(-0.5, -1.0, -0.5),
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
        }
    }
}

/// A point light — emits light in all directions from a position.
///
/// Pair with [`Transform`](crate::math::Transform) for position.
/// Up to 8 point lights are supported per scene.
#[derive(Debug)]
pub struct PointLight {
    /// Light color (linear RGB).
    pub color: [f32; 3],
    /// Intensity multiplier.
    pub intensity: f32,
    /// Maximum radius of influence. Light falls off to zero at this distance.
    pub radius: f32,
}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            radius: 10.0,
        }
    }
}

/// Ambient light resource (singleton, not per-entity).
///
/// A constant amount of light applied to all surfaces regardless of
/// orientation. Prevents fully-black shadows. Insert as a resource:
/// ```ignore
/// world.insert_resource(AmbientLight { intensity: 0.1, ..Default::default() });
/// ```
#[derive(Debug)]
pub struct AmbientLight {
    /// Light color (linear RGB).
    pub color: [f32; 3],
    /// Intensity multiplier.
    pub intensity: f32,
}

impl Default for AmbientLight {
    fn default() -> Self {
        Self {
            color: [1.0, 1.0, 1.0],
            intensity: 0.1,
        }
    }
}

// ── Convenience constructors ──

/// Create a [`Mesh3d`] referencing the built-in cube.
pub fn cube_mesh() -> Mesh3d {
    Mesh3d { mesh: mesh_cube() }
}

/// Create a [`Mesh3d`] referencing the built-in plane.
pub fn plane_mesh() -> Mesh3d {
    Mesh3d { mesh: mesh_plane() }
}

/// Create a [`Mesh3d`] referencing the built-in sphere.
pub fn sphere_mesh() -> Mesh3d {
    Mesh3d { mesh: mesh_sphere() }
}

/// Create a [`Mesh3d`] referencing the built-in cylinder.
pub fn cylinder_mesh() -> Mesh3d {
    Mesh3d { mesh: mesh_cylinder() }
}
