//! Keyboard and mouse input state.
//!
//! The [`Input`] resource tracks which keys/buttons are currently pressed,
//! just pressed this frame, or just released this frame.
//!
//! Updated by the window event handler each frame.

use std::collections::HashSet;
use std::hash::Hash;

pub use winit::keyboard::KeyCode;
pub use winit::event::MouseButton;

/// Tracks the state of a set of inputs (keys or mouse buttons).
///
/// - `pressed`: currently held down
/// - `just_pressed`: pressed this frame (not held last frame)
/// - `just_released`: released this frame
pub struct Input<T: Eq + Hash + Copy> {
    pressed: HashSet<T>,
    just_pressed: HashSet<T>,
    just_released: HashSet<T>,
}

impl<T: Eq + Hash + Copy> Input<T> {
    pub fn new() -> Self {
        Self {
            pressed: HashSet::new(),
            just_pressed: HashSet::new(),
            just_released: HashSet::new(),
        }
    }

    /// Returns `true` if the input is currently held down.
    pub fn pressed(&self, input: T) -> bool {
        self.pressed.contains(&input)
    }

    /// Returns `true` if the input was pressed this frame.
    pub fn just_pressed(&self, input: T) -> bool {
        self.just_pressed.contains(&input)
    }

    /// Returns `true` if the input was released this frame.
    pub fn just_released(&self, input: T) -> bool {
        self.just_released.contains(&input)
    }

    /// Call when an input is pressed (from event handler).
    pub(crate) fn press(&mut self, input: T) {
        if self.pressed.insert(input) {
            self.just_pressed.insert(input);
        }
    }

    /// Call when an input is released (from event handler).
    pub(crate) fn release(&mut self, input: T) {
        if self.pressed.remove(&input) {
            self.just_released.insert(input);
        }
    }

    /// Clear per-frame state. Called at the start of each frame.
    pub(crate) fn clear_just(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }
}

impl<T: Eq + Hash + Copy> Default for Input<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Mouse cursor position in window coordinates.
#[derive(Debug, Clone, Copy, Default)]
pub struct CursorPosition {
    pub x: f32,
    pub y: f32,
}
