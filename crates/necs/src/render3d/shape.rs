//! # Shape3d — First-Class 3D Shape Primitives
//!
//! Draw 3D primitives with a single component instead of requiring separate
//! `Mesh3d` + `Material`. Each shape maps to a built-in unit mesh and applies
//! scaling to achieve the requested dimensions.
//!
//! ```ignore
//! world.spawn((
//!     Transform::from_xyz(0.0, 1.0, 0.0),
//!     Shape3d::sphere(0.5).color([1.0, 0.0, 0.0, 1.0]),
//! ));
//! ```

use super::mesh::{mesh_cube, mesh_cylinder, mesh_plane, mesh_sphere, MeshHandle};

/// The kind and dimensions of a 3D shape primitive.
#[derive(Debug, Clone)]
pub enum ShapeKind3d {
    Sphere { radius: f32 },
    Cuboid { width: f32, height: f32, depth: f32 },
    Cylinder { radius: f32, height: f32 },
    Plane { width: f32, depth: f32 },
}

/// A 3D shape component. Pair with [`Transform`](crate::math::Transform) to render.
///
/// Maps to a built-in mesh handle and applies implicit scaling based on shape
/// dimensions. Material properties (color, metallic, roughness) are embedded.
#[derive(Debug, Clone)]
pub struct Shape3d {
    pub kind: ShapeKind3d,
    /// Base color (linear RGBA).
    pub base_color: [f32; 4],
    /// Metallic factor [0.0, 1.0].
    pub metallic: f32,
    /// Roughness factor [0.0, 1.0].
    pub roughness: f32,
}

impl Shape3d {
    /// A sphere with the given radius. Default white, non-metallic, roughness 0.5.
    pub fn sphere(radius: f32) -> Self {
        Self {
            kind: ShapeKind3d::Sphere { radius },
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic: 0.0,
            roughness: 0.5,
        }
    }

    /// A box (cuboid) with the given half-extents expressed as full width, height, depth.
    pub fn cuboid(width: f32, height: f32, depth: f32) -> Self {
        Self {
            kind: ShapeKind3d::Cuboid { width, height, depth },
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic: 0.0,
            roughness: 0.5,
        }
    }

    /// A cylinder along the Y axis with the given radius and total height.
    pub fn cylinder(radius: f32, height: f32) -> Self {
        Self {
            kind: ShapeKind3d::Cylinder { radius, height },
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic: 0.0,
            roughness: 0.5,
        }
    }

    /// A plane on the XZ plane with the given width and depth.
    pub fn plane(width: f32, depth: f32) -> Self {
        Self {
            kind: ShapeKind3d::Plane { width, depth },
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic: 0.0,
            roughness: 0.5,
        }
    }

    /// Set the base color (linear RGBA).
    pub fn color(mut self, rgba: [f32; 4]) -> Self {
        self.base_color = rgba;
        self
    }

    /// Set the metallic factor.
    pub fn metallic(mut self, m: f32) -> Self {
        self.metallic = m;
        self
    }

    /// Set the roughness factor.
    pub fn roughness(mut self, r: f32) -> Self {
        self.roughness = r;
        self
    }

    /// Returns the built-in mesh handle for this shape's kind.
    pub(crate) fn mesh_handle(&self) -> MeshHandle {
        match &self.kind {
            ShapeKind3d::Sphere { .. } => mesh_sphere(),
            ShapeKind3d::Cuboid { .. } => mesh_cube(),
            ShapeKind3d::Cylinder { .. } => mesh_cylinder(),
            ShapeKind3d::Plane { .. } => mesh_plane(),
        }
    }

    /// Returns the scale factor to apply to the unit mesh to get the requested dimensions.
    ///
    /// Built-in meshes are unit-sized:
    /// - Sphere: radius 0.5 → scale = radius / 0.5 = radius * 2
    /// - Cube: side 1.0 → scale = (width, height, depth)
    /// - Cylinder: radius 0.5, height 1.0 → scale = (radius*2, height, radius*2)
    /// - Plane: side 1.0 → scale = (width, 1.0, depth)
    pub(crate) fn shape_scale(&self) -> glam::Vec3 {
        match &self.kind {
            ShapeKind3d::Sphere { radius } => glam::Vec3::splat(radius * 2.0),
            ShapeKind3d::Cuboid { width, height, depth } => glam::Vec3::new(*width, *height, *depth),
            ShapeKind3d::Cylinder { radius, height } => glam::Vec3::new(radius * 2.0, *height, radius * 2.0),
            ShapeKind3d::Plane { width, depth } => glam::Vec3::new(*width, 1.0, *depth),
        }
    }
}
