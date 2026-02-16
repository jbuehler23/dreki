//! # Render2d — 2D Sprite Rendering
//!
//! A 2D sprite renderer turns a collection of textured (or solid-colored)
//! rectangles into pixels on screen. Each sprite is a *quad* — four vertices
//! forming a rectangle — that can be positioned, rotated, and scaled anywhere
//! in 2D space. The camera determines which portion of the world is visible.
//!
//! ## Architecture
//!
//! Every frame follows a five-stage pipeline:
//!
//! ```text
//!  Camera2d + Transform              Sprite + Transform
//!         │                            │  │  │  ...
//!         ▼                            ▼  ▼  ▼
//!   ┌──────────┐    ┌────────────────────────────────┐
//!   │ compute  │    │         collect sprites         │
//!   │ view-    │    │  emit 4 vertices per quad,      │
//!   │ projection│    │  apply model matrix CPU-side   │
//!   └────┬─────┘    └───────────────┬────────────────┘
//!        │                          │
//!        │                          ▼
//!        │                 ┌─────────────────┐
//!        │                 │  Z-sort (back    │
//!        │                 │  to front)       │
//!        │                 └────────┬────────┘
//!        │                          │
//!        │                          ▼
//!        │                 ┌─────────────────┐
//!        │                 │  batch by        │
//!        │                 │  texture         │
//!        │                 └────────┬────────┘
//!        │                          │
//!        ▼                          ▼
//!   ┌──────────────────────────────────────┐
//!   │  GPU render pass                      │
//!   │  • upload VP uniform + vertex/index   │
//!   │  • one draw call per texture batch    │
//!   └──────────────────────────────────────┘
//! ```
//!
//! ## Design Decisions
//!
//! **CPU-side vertex transform.** Each sprite's model matrix (position, rotation,
//! scale) is multiplied into its four vertex positions on the CPU *before*
//! uploading to the GPU. The shader then only multiplies by the camera's
//! view-projection matrix. This trades a small amount of CPU work for a major
//! simplification: sprites with different transforms but the same texture can
//! share a single draw call. The alternative — passing a per-sprite model matrix
//! as a uniform or instance attribute — would require either one draw call per
//! sprite or instanced rendering with a separate transform buffer.
//!
//! **Painter's algorithm.** Sprites are Z-sorted back-to-front and drawn in
//! that order with alpha blending. There is no depth buffer. This is the
//! simplest correct approach for 2D with transparency: a depth buffer would
//! discard fragments behind already-drawn pixels, but semi-transparent sprites
//! need to blend with whatever is behind them. The tradeoff is O(n log n)
//! sorting per frame, which is negligible for typical 2D sprite counts.
//!
//! **Texture batching.** After sorting, consecutive sprites that share the same
//! texture are drawn in a single `draw_indexed` call. Switching textures
//! requires changing the GPU bind group (an expensive operation relative to
//! just adding more vertices to an existing draw). For games with many sprites,
//! using a texture atlas (packing many images into one texture) would further
//! reduce batch breaks, but that's a future optimization.
//!
//! ## Comparison
//!
//! - **Bevy** (`bevy_sprite`): Uses instanced rendering with a per-instance
//!   transform buffer, automatic texture atlasing, and a depth buffer with a
//!   specialized shader for order-independent 2D. Far more complex but scales
//!   to thousands of sprites.
//! - **Macroquad**: Similar CPU-side approach — immediate-mode API that builds
//!   vertex buffers each frame. No explicit batching; relies on draw-call
//!   ordering.
//! - **Love2D**: Lua-level `love.graphics.draw()` calls map to individual draw
//!   commands; the C++ backend does automatic batching of consecutive same-
//!   texture draws, very similar to our approach.

pub(crate) mod batch;
pub(crate) mod draw;
pub mod font;
pub(crate) mod pipeline;
pub mod shapes;
pub(crate) mod texture;
pub(crate) mod vertex;

#[cfg(feature = "physics2d")]
pub(crate) mod debug_wireframe;

#[cfg(feature = "physics2d")]
pub use debug_wireframe::DebugColliders2d;
pub use font::{FontHandle, Text, load_font};
pub use shapes::{Shape2d, ShapeKind2d};
pub use texture::{TextureHandle, create_texture_from_rgba, load_texture};

use crate::math::{Rect, Vec2};

/// Marker component for a 2D camera. Pair with [`Transform`](crate::math::Transform).
///
/// The camera produces an orthographic projection where 1 world unit = 1 pixel
/// at the default zoom. The origin is at the center of the screen.
#[derive(Debug)]
pub struct Camera2d;

/// A 2D sprite component. Pair with [`Transform`](crate::math::Transform).
///
/// Without a texture, the sprite renders as a solid colored quad using the
/// built-in 1x1 white texture.
#[derive(Debug)]
pub struct Sprite {
    /// Texture to draw. `None` uses the built-in 1x1 white texture.
    pub texture: Option<TextureHandle>,
    /// Tint color multiplied with the texture sample.
    pub color: Color,
    /// Size in world units. If zero, auto-sized from texture dimensions.
    pub size: Vec2,
    /// Flip the sprite horizontally.
    pub flip_x: bool,
    /// Flip the sprite vertically.
    pub flip_y: bool,
    /// UV sub-region of the texture. Defaults to full texture.
    pub texture_rect: Rect,
}

impl Sprite {
    /// Create a new sprite with default values (white, zero-sized).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the tint color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set the size in world units.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = Vec2::new(width, height);
        self
    }

    /// Set the texture.
    pub fn texture(mut self, texture: TextureHandle) -> Self {
        self.texture = Some(texture);
        self
    }
}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            texture: None,
            color: Color::WHITE,
            size: Vec2::ZERO,
            flip_x: false,
            flip_y: false,
            texture_rect: Rect::FULL,
        }
    }
}

/// An RGBA color with floating-point components in [0, 1].
#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const RED: Self = Self { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const GREEN: Self = Self { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };
    pub const BLUE: Self = Self { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };

    /// Create a color from RGB (alpha = 1).
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Create a color from RGBA.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub(crate) fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}
