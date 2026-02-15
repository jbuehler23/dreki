//! # Animation — Sprite Sheets and Property Tweening
//!
//! Two complementary systems for animating 2D entities:
//!
//! ## Sprite Sheet Animation
//!
//! A sprite sheet is a single texture image containing a grid of frames (e.g. a
//! character walk cycle). [`SpriteSheet`] describes the grid layout,
//! [`AnimationClip`] defines which frames to play and at what speed, and
//! [`AnimationPlayer`] is a component that drives playback. The
//! [`animate_sprites`] system advances all players each frame and updates
//! `Sprite.texture_rect` to show the correct frame.
//!
//! ```text
//!  ┌────┬────┬────┬────┐
//!  │ 0  │ 1  │ 2  │ 3  │   4-column, 2-row sprite sheet
//!  ├────┼────┼────┼────┤   frame index = row * columns + column
//!  │ 4  │ 5  │ 6  │ 7  │
//!  └────┴────┴────┴────┘
//! ```
//!
//! ## Property Tweening
//!
//! [`Tween`] interpolates a single property (position, scale, rotation, or
//! color channel) from a start value to an end value over a duration, using an
//! [`EaseFunction`] curve. Supports looping and ping-pong modes. The
//! [`advance_tweens`] system applies the interpolated values each frame.

use crate::ecs::World;
use crate::math::{Rect, Transform, Vec2};
use crate::render2d::{Color, Sprite};
use crate::time::Time;

// ---------------------------------------------------------------------------
// Sprite Sheet Animation
// ---------------------------------------------------------------------------

/// Describes a uniform grid sprite sheet.
///
/// All frames must be the same size. Frame indices are row-major (left-to-right,
/// top-to-bottom). Supports optional padding between frames and an offset from
/// the top-left corner of the texture.
#[derive(Debug, Clone)]
pub struct SpriteSheet {
    pub columns: u32,
    pub rows: u32,
    /// Size of one frame in pixels.
    pub tile_size: Vec2,
    /// Space between frames in pixels (default: zero).
    pub padding: Vec2,
    /// Top-left margin in pixels (default: zero).
    pub offset: Vec2,
    /// Total texture dimensions in pixels.
    pub texture_size: Vec2,
}

impl SpriteSheet {
    /// Create a sprite sheet from grid dimensions and texture size.
    ///
    /// Frame size is computed as `texture_size / grid`. No padding or offset.
    pub fn new(columns: u32, rows: u32, texture_size: Vec2) -> Self {
        Self {
            columns,
            rows,
            tile_size: Vec2::new(
                texture_size.x / columns as f32,
                texture_size.y / rows as f32,
            ),
            padding: Vec2::ZERO,
            offset: Vec2::ZERO,
            texture_size,
        }
    }

    /// Create a sprite sheet with explicit tile size, optional padding and offset.
    ///
    /// The total texture size is computed from the grid parameters. If your
    /// texture has extra blank space beyond the grid, set `texture_size`
    /// directly on the returned struct.
    pub fn from_grid(
        tile_size: Vec2,
        columns: u32,
        rows: u32,
        padding: Option<Vec2>,
        offset: Option<Vec2>,
    ) -> Self {
        let padding = padding.unwrap_or(Vec2::ZERO);
        let offset = offset.unwrap_or(Vec2::ZERO);
        let texture_size = Vec2::new(
            offset.x + columns as f32 * tile_size.x + (columns - 1) as f32 * padding.x,
            offset.y + rows as f32 * tile_size.y + (rows - 1) as f32 * padding.y,
        );
        Self {
            columns,
            rows,
            tile_size,
            padding,
            offset,
            texture_size,
        }
    }

    /// Returns the UV [`Rect`] for a given frame index (row-major, 0-based).
    pub fn frame_rect(&self, index: u32) -> Rect {
        let col = index % self.columns;
        let row = index / self.columns;
        let x = self.offset.x + col as f32 * (self.tile_size.x + self.padding.x);
        let y = self.offset.y + row as f32 * (self.tile_size.y + self.padding.y);
        Rect::from_pixels(
            x,
            y,
            self.tile_size.x,
            self.tile_size.y,
            self.texture_size.x,
            self.texture_size.y,
        )
    }

    /// Total number of frames in the sheet.
    pub fn frame_count(&self) -> u32 {
        self.columns * self.rows
    }

    /// Play all frames at the given speed. Consumes the sheet.
    pub fn play_all(self, frame_time: f32) -> AnimationPlayer {
        let clip = AnimationClip::from_range(0, self.frame_count() - 1, frame_time);
        AnimationPlayer::new(self, clip)
    }

    /// Play frames `first..=last` at the given speed. Consumes the sheet.
    pub fn play_range(self, first: u32, last: u32, frame_time: f32) -> AnimationPlayer {
        let clip = AnimationClip::from_range(first, last, frame_time);
        AnimationPlayer::new(self, clip)
    }
}

/// A sequence of frames with playback settings.
#[derive(Debug, Clone)]
pub struct AnimationClip {
    /// Frame indices into the sprite sheet (row-major order).
    pub frames: Vec<u32>,
    /// Seconds per frame.
    pub frame_time: f32,
    /// Whether to loop when the last frame is reached.
    pub looping: bool,
}

impl AnimationClip {
    /// Play frames `first..=last` sequentially.
    pub fn from_range(first: u32, last: u32, frame_time: f32) -> Self {
        Self {
            frames: (first..=last).collect(),
            frame_time,
            looping: false,
        }
    }

    /// Play all frames in the given sprite sheet (0 through `frame_count - 1`).
    pub fn from_sheet(sheet: &SpriteSheet, frame_time: f32) -> Self {
        Self {
            frames: (0..sheet.frame_count()).collect(),
            frame_time,
            looping: false,
        }
    }

    /// Enable looping (builder pattern).
    pub fn looping(mut self) -> Self {
        self.looping = true;
        self
    }
}

/// Component: drives sprite-sheet animation on an entity.
///
/// Attach alongside a [`Sprite`] (with a sprite-sheet texture) and add the
/// [`animate_sprites`] system to your schedule.
#[derive(Debug)]
pub struct AnimationPlayer {
    pub sheet: SpriteSheet,
    pub clip: AnimationClip,
    /// Accumulated time within the current frame.
    pub timer: f32,
    /// Index into `clip.frames`.
    pub current_index: usize,
    /// Set to `true` when a non-looping clip reaches the last frame.
    pub finished: bool,
    /// Playback speed multiplier (1.0 = normal).
    pub speed: f32,
}

impl AnimationPlayer {
    pub fn new(sheet: SpriteSheet, clip: AnimationClip) -> Self {
        Self {
            sheet,
            clip,
            timer: 0.0,
            current_index: 0,
            finished: false,
            speed: 1.0,
        }
    }

    /// Enable looping on the current clip (builder pattern).
    pub fn looping(mut self) -> Self {
        self.clip.looping = true;
        self
    }

    /// Set playback speed multiplier (builder pattern).
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    /// Replace the current clip and reset playback.
    pub fn play(&mut self, clip: AnimationClip) {
        self.clip = clip;
        self.timer = 0.0;
        self.current_index = 0;
        self.finished = false;
    }

    /// Get the current frame's UV rect.
    pub fn current_rect(&self) -> Rect {
        let frame = self.clip.frames[self.current_index];
        self.sheet.frame_rect(frame)
    }
}

/// System: advance sprite-sheet animations and update `Sprite.texture_rect`.
pub fn animate_sprites(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();

    world.query::<(&mut AnimationPlayer, &mut Sprite)>(|_entity, (player, sprite)| {
        if player.finished || player.clip.frames.is_empty() {
            return;
        }

        player.timer += dt * player.speed;

        while player.timer >= player.clip.frame_time {
            player.timer -= player.clip.frame_time;
            player.current_index += 1;

            if player.current_index >= player.clip.frames.len() {
                if player.clip.looping {
                    player.current_index = 0;
                } else {
                    player.current_index = player.clip.frames.len() - 1;
                    player.finished = true;
                    break;
                }
            }
        }

        sprite.texture_rect = player.current_rect();
    });
}

// ---------------------------------------------------------------------------
// Property Tweening
// ---------------------------------------------------------------------------

/// Standard easing curves.
///
/// Each variant maps `t` in \[0, 1\] to an eased value in roughly \[0, 1\].
#[derive(Debug, Clone, Copy)]
pub enum EaseFunction {
    Linear,
    QuadIn,
    QuadOut,
    QuadInOut,
    CubicIn,
    CubicOut,
    CubicInOut,
    SineIn,
    SineOut,
    SineInOut,
}

impl EaseFunction {
    /// Evaluate the easing function at `t` (clamped to \[0, 1\]).
    pub fn sample(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::QuadIn => t * t,
            Self::QuadOut => 1.0 - (1.0 - t) * (1.0 - t),
            Self::QuadInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            Self::CubicIn => t * t * t,
            Self::CubicOut => 1.0 - (1.0 - t).powi(3),
            Self::CubicInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Self::SineIn => 1.0 - (t * std::f32::consts::FRAC_PI_2).cos(),
            Self::SineOut => (t * std::f32::consts::FRAC_PI_2).sin(),
            Self::SineInOut => -(std::f32::consts::PI * t).cos() / 2.0 + 0.5,
        }
    }
}

/// What property a [`Tween`] interpolates.
#[derive(Debug, Clone, Copy)]
pub enum TweenTarget {
    TranslationX { start: f32, end: f32 },
    TranslationY { start: f32, end: f32 },
    TranslationZ { start: f32, end: f32 },
    ScaleUniform { start: f32, end: f32 },
    /// Z-axis rotation in radians (2D rotation).
    Rotation { start: f32, end: f32 },
    ColorR { start: f32, end: f32 },
    ColorG { start: f32, end: f32 },
    ColorB { start: f32, end: f32 },
    ColorA { start: f32, end: f32 },
}

/// Component: interpolates a single property over time with easing.
///
/// Attach to an entity that has the target component ([`Transform`] for
/// position/scale/rotation, [`Sprite`] for color) and add [`advance_tweens`]
/// to your schedule.
#[derive(Debug)]
pub struct Tween {
    pub target: TweenTarget,
    pub ease: EaseFunction,
    pub duration: f32,
    pub elapsed: f32,
    pub looping: bool,
    pub ping_pong: bool,
    /// `true` when moving backward in ping-pong mode.
    pub reversing: bool,
    pub finished: bool,
}

impl Tween {
    pub fn new(target: TweenTarget, ease: EaseFunction, duration: f32) -> Self {
        Self {
            target,
            ease,
            duration,
            elapsed: 0.0,
            looping: false,
            ping_pong: false,
            reversing: false,
            finished: false,
        }
    }

    /// Enable looping (restarts from the beginning when finished).
    pub fn looping(mut self) -> Self {
        self.looping = true;
        self
    }

    /// Enable ping-pong mode (reverses direction at each end, implies looping).
    pub fn ping_pong(mut self) -> Self {
        self.ping_pong = true;
        self.looping = true;
        self
    }
}

/// Advance the tween timer, handling loop and ping-pong logic.
fn advance_tween_timer(tween: &mut Tween, dt: f32) {
    tween.elapsed += dt;

    if tween.elapsed >= tween.duration {
        if tween.ping_pong {
            tween.elapsed -= tween.duration;
            tween.reversing = !tween.reversing;
        } else if tween.looping {
            tween.elapsed -= tween.duration;
        } else {
            tween.elapsed = tween.duration;
            tween.finished = true;
        }
    }
}

/// Compute the eased `t` value, respecting ping-pong direction.
fn eased_t(tween: &Tween) -> f32 {
    let raw = if tween.duration > 0.0 {
        (tween.elapsed / tween.duration).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let directed = if tween.reversing { 1.0 - raw } else { raw };
    tween.ease.sample(directed)
}

/// Linearly interpolate between `a` and `b` at factor `t`.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Apply a tween value to Transform fields (translation, scale, rotation).
fn apply_transform_tween(target: &TweenTarget, t: f32, transform: &mut Transform) {
    match *target {
        TweenTarget::TranslationX { start, end } => {
            transform.translation.x = lerp(start, end, t);
        }
        TweenTarget::TranslationY { start, end } => {
            transform.translation.y = lerp(start, end, t);
        }
        TweenTarget::TranslationZ { start, end } => {
            transform.translation.z = lerp(start, end, t);
        }
        TweenTarget::ScaleUniform { start, end } => {
            let s = lerp(start, end, t);
            transform.scale = glam::Vec3::splat(s);
        }
        TweenTarget::Rotation { start, end } => {
            let angle = lerp(start, end, t);
            transform.rotation = glam::Quat::from_rotation_z(angle);
        }
        // Color targets are handled in apply_color_tween
        _ => {}
    }
}

/// Apply a tween value to Color fields.
fn apply_color_tween(target: &TweenTarget, t: f32, color: &mut Color) {
    match *target {
        TweenTarget::ColorR { start, end } => color.r = lerp(start, end, t),
        TweenTarget::ColorG { start, end } => color.g = lerp(start, end, t),
        TweenTarget::ColorB { start, end } => color.b = lerp(start, end, t),
        TweenTarget::ColorA { start, end } => color.a = lerp(start, end, t),
        // Transform targets are handled in apply_transform_tween
        _ => {}
    }
}

// ── Plugin ───────────────────────────────────────────────────────────

/// Plugin: registers the [`animate_sprites`] and [`advance_tweens`] systems.
///
/// Included automatically by [`DefaultPlugins`](crate::app::DefaultPlugins).
/// Can also be added explicitly if you are not using `DefaultPlugins`.
pub struct AnimationPlugin;

impl crate::app::Plugin for AnimationPlugin {
    fn build(&self, app: &mut crate::app::App) {
        app.systems.add_system(animate_sprites);
        app.systems.add_system(advance_tweens);
    }
}

/// System: advance tweens and apply interpolated values to Transform/Sprite.
///
/// Entities with `Tween` + `Transform` get transform properties applied.
/// Entities with `Tween` + `Sprite` get color properties applied.
pub fn advance_tweens(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();

    // Pass 1: advance timers + apply to Transform
    world.query::<(&mut Tween, &mut Transform)>(|_entity, (tween, transform)| {
        if tween.finished {
            return;
        }
        advance_tween_timer(tween, dt);
        let t = eased_t(tween);
        apply_transform_tween(&tween.target, t, transform);
    });

    // Pass 2: apply to Sprite color (timer already advanced in pass 1)
    world.query::<(&Tween, &mut Sprite)>(|_entity, (tween, sprite)| {
        if tween.finished {
            return;
        }
        let t = eased_t(tween);
        apply_color_tween(&tween.target, t, &mut sprite.color);
    });
}
