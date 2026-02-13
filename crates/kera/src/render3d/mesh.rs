//! # Mesh — GPU Mesh Storage
//!
//! A *mesh* is a collection of vertices and indices that define a 3D shape.
//! This module manages uploading mesh data to the GPU and provides
//! [`MeshHandle`] — a lightweight, copyable reference to a loaded mesh.
//!
//! ## The Handle Pattern
//!
//! Same idea as [`TextureHandle`](super::texture::TextureHandle3d): users hold
//! a cheap index into the [`MeshStore`], never a raw GPU buffer. This keeps
//! components `Copy`-able and decouples mesh lifetime from entity lifetime.
//!
//! ## Built-In Meshes
//!
//! When the `MeshStore` is created, it pre-uploads four primitives from
//! [`shapes`](super::shapes):
//!
//! | Handle | Shape    | Vertices | Indices |
//! |--------|----------|----------|---------|
//! | 0      | Cube     | 24       | 36      |
//! | 1      | Plane    | 4        | 6       |
//! | 2      | Sphere   | 561      | 3072    |
//! | 3      | Cylinder | 228      | 576     |
//!
//! These are always available — no loading needed.
//!
//! ## GpuMesh
//!
//! Each uploaded mesh becomes a [`GpuMesh`]: a vertex buffer, an index buffer,
//! and an index count. During rendering, the draw call binds these buffers
//! and issues `draw_indexed(0..index_count)`.
//!
//! ## Comparison
//!
//! - **Bevy**: `Mesh` is a CPU-side struct with attribute arrays; `GpuMesh`
//!   is the uploaded version. A `RenderAsset` pipeline handles the upload.
//! - **three.js**: `BufferGeometry` holds typed arrays that are uploaded
//!   lazily when first rendered.

use wgpu::util::DeviceExt;

use super::shapes;
use super::vertex::MeshVertex;
use crate::render::GpuContext;

/// Handle to a mesh in the [`MeshStore`]. Lightweight and `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshHandle(pub(crate) usize);

/// A mesh that has been uploaded to GPU buffers.
pub(crate) struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

/// Stores all uploaded meshes. Pre-populated with built-in primitives.
pub(crate) struct MeshStore {
    meshes: Vec<GpuMesh>,
}

impl MeshStore {
    /// Create a new store and upload the built-in primitives.
    pub fn new(gpu: &GpuContext) -> Self {
        let mut store = Self {
            meshes: Vec::new(),
        };

        // Built-in primitives: cube(0), plane(1), sphere(2), cylinder(3)
        let (cube_v, cube_i) = shapes::cube();
        store.upload(gpu, &cube_v, &cube_i);

        let (plane_v, plane_i) = shapes::plane();
        store.upload(gpu, &plane_v, &plane_i);

        let (sphere_v, sphere_i) = shapes::sphere(32, 16);
        store.upload(gpu, &sphere_v, &sphere_i);

        let (cyl_v, cyl_i) = shapes::cylinder(0.5, 0.5, 32);
        store.upload(gpu, &cyl_v, &cyl_i);

        store
    }

    /// Upload mesh data to the GPU and return a handle.
    pub fn upload(&mut self, gpu: &GpuContext, vertices: &[MeshVertex], indices: &[u32]) -> MeshHandle {
        let vertex_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh vertex buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh index buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let handle = MeshHandle(self.meshes.len());
        self.meshes.push(GpuMesh {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
        });
        handle
    }

    /// Get the GPU mesh for a handle.
    pub fn get(&self, handle: MeshHandle) -> &GpuMesh {
        &self.meshes[handle.0]
    }
}

/// Well-known handle for the built-in cube mesh.
pub(crate) fn mesh_cube() -> MeshHandle {
    MeshHandle(0)
}

/// Well-known handle for the built-in plane mesh.
pub(crate) fn mesh_plane() -> MeshHandle {
    MeshHandle(1)
}

/// Well-known handle for the built-in sphere mesh.
pub(crate) fn mesh_sphere() -> MeshHandle {
    MeshHandle(2)
}

/// Well-known handle for the built-in cylinder mesh.
pub(crate) fn mesh_cylinder() -> MeshHandle {
    MeshHandle(3)
}
