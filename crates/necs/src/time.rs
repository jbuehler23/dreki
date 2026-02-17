//! Frame timing and delta time.
//!
//! The [`Time`] resource is updated by the framework at the start of each
//! frame. Systems can read it to get frame delta time and total elapsed time.

use std::time::{Duration, Instant};

/// Frame timing resource. Inserted by the framework and updated each frame.
#[derive(Clone, Copy)]
pub struct Time {
    /// When the app started.
    startup: Instant,
    /// When the current frame started.
    frame_start: Instant,
    /// Duration of the previous frame.
    delta: Duration,
    /// Total time since app startup.
    elapsed: Duration,
    /// Frame counter.
    frame_count: u64,
}

impl Time {
    pub(crate) fn new() -> Self {
        let now = Instant::now();
        Self {
            startup: now,
            frame_start: now,
            delta: Duration::ZERO,
            elapsed: Duration::ZERO,
            frame_count: 0,
        }
    }

    /// Call at the start of each frame to update timing.
    pub(crate) fn update(&mut self) {
        let now = Instant::now();
        self.delta = now - self.frame_start;
        self.frame_start = now;
        self.elapsed = now - self.startup;
        self.frame_count += 1;
    }

    /// Duration of the previous frame.
    pub fn delta(&self) -> Duration {
        self.delta
    }

    /// Delta time in seconds (f32), the most common way to use it.
    pub fn delta_secs(&self) -> f32 {
        self.delta.as_secs_f32()
    }

    /// Total elapsed time since app start.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Total elapsed time in seconds (f32).
    pub fn elapsed_secs(&self) -> f32 {
        self.elapsed.as_secs_f32()
    }

    /// Number of frames rendered so far.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Estimated FPS based on the last frame's delta.
    pub fn fps(&self) -> f32 {
        if self.delta.as_secs_f32() > 0.0 {
            1.0 / self.delta.as_secs_f32()
        } else {
            0.0
        }
    }
}
