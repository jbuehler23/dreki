//! Math types and glam re-exports.
//!
//! We re-export [glam](https://docs.rs/glam) types so users don't need to
//! depend on it directly. The [`Transform`] type provides position, rotation,
//! and scale for 2D and 3D entities.

pub use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

/// A 3D transform: position, rotation, and scale.
///
/// Works for both 2D and 3D — 2D entities just ignore the Z axis.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    /// Identity transform (origin, no rotation, uniform scale of 1).
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    /// Create a transform at the given position.
    pub fn from_xyz(x: f32, y: f32, z: f32) -> Self {
        Self {
            translation: Vec3::new(x, y, z),
            ..Self::IDENTITY
        }
    }

    /// Create a transform at the given 2D position (z = 0).
    pub fn from_xy(x: f32, y: f32) -> Self {
        Self::from_xyz(x, y, 0.0)
    }

    /// Create a transform that looks at a target point from the current position.
    ///
    /// Useful for camera placement: `Transform::from_xyz(0, 5, 10).looking_at(Vec3::ZERO, Vec3::Y)`
    /// creates a camera at (0,5,10) looking toward the origin.
    pub fn looking_at(mut self, target: Vec3, up: Vec3) -> Self {
        let forward = (target - self.translation).normalize();
        self.rotation = Quat::from_rotation_arc(Vec3::NEG_Z, forward);
        // Recompute to respect the up vector properly
        let look = Mat4::look_at_rh(self.translation, target, up);
        let (_, rot, _) = look.inverse().to_scale_rotation_translation();
        self.rotation = rot;
        self
    }

    /// Return a copy with uniform scale applied.
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.scale = Vec3::splat(scale);
        self
    }

    /// Compute the 4x4 model matrix.
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// A normalized rectangle within a texture (UV space, 0.0–1.0).
///
/// Used to select a sub-region of a texture for rendering — for example, a
/// single frame from a sprite sheet. Coordinates are in UV space where (0,0) is
/// the top-left corner and (1,1) is the bottom-right corner.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    /// The full texture (0,0) to (1,1).
    pub const FULL: Self = Self {
        min: Vec2::ZERO,
        max: Vec2::ONE,
    };

    /// Build from pixel coordinates and texture dimensions.
    pub fn from_pixels(x: f32, y: f32, w: f32, h: f32, tex_w: f32, tex_h: f32) -> Self {
        Self {
            min: Vec2::new(x / tex_w, y / tex_h),
            max: Vec2::new((x + w) / tex_w, (y + h) / tex_h),
        }
    }
}

impl Default for Rect {
    fn default() -> Self {
        Self::FULL
    }
}
