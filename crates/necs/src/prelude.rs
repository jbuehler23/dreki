//! Convenience re-exports — `use necs::prelude::*` for the common items.
//!
//! Types only — all functionality is discoverable through methods on types,
//! not free functions.

// Core
pub use crate::asset::AssetServer;
pub use crate::context::{Context, EntityBuilder, InputState};
pub use crate::ecs::{Children, Entity, GlobalTransform, Parent, World};
pub use crate::game::{Game, Plugin};
pub use crate::input::{CursorPosition, Input, KeyCode, MouseButton};
pub use crate::math::{Mat4, Quat, Rect, Transform, Vec2, Vec3, Vec4};
pub use crate::render::{ClearColor, GpuContext};
pub use crate::scene::{SceneData, SceneMarker, SceneRegistry};
pub use crate::scene_builder::{SceneBuilder, SceneManager, Scenes, Template};
pub use crate::time::Time;

// Render 2D (feature-gated)
#[cfg(feature = "render2d")]
pub use crate::animation::{
    AnimationClip, AnimationPlayer, EaseFunction, SpriteSheet, Tween, TweenTarget,
};
#[cfg(feature = "render2d")]
pub use crate::render2d::{Camera2d, Color, FontHandle, Shape2d, ShapeKind2d, Sprite, Text, TextureHandle};

// Render 3D (feature-gated)
#[cfg(feature = "render3d")]
pub use crate::render3d::{
    AmbientLight, Camera3d, DirectionalLight, Material, Mesh3d, MeshHandle, PointLight,
    Shape3d, ShapeKind3d, TextureHandle3d,
};

// Debug colliders
#[cfg(all(feature = "render2d", feature = "physics2d"))]
pub use crate::render2d::DebugColliders2d;
#[cfg(all(feature = "render3d", feature = "physics3d"))]
pub use crate::render3d::DebugColliders3d;

// Audio (feature-gated)
#[cfg(feature = "audio")]
pub use crate::audio::{Audio, AudioEngine, AudioError, AudioSource, SoundData, SoundHandle};

// Physics (feature-gated)
#[cfg(feature = "physics2d")]
pub use crate::physics2d::{
    Collider2d, ColliderShape2d, Physics2d, PhysicsWorld2d, RigidBody2d, RigidBodyType2d,
};
#[cfg(feature = "physics3d")]
pub use crate::physics3d::{
    Collider3d, ColliderShape3d, Physics3d, PhysicsWorld3d, RigidBody3d, RigidBodyType3d,
};

// Diagnostics (feature-gated)
#[cfg(feature = "diagnostics")]
pub use crate::diag::ComponentRegistry;
