//! Convenience re-exports — `use kera::prelude::*` for the common items.

pub use crate::app::{App, DefaultPlugins, Plugin};
pub use crate::asset::AssetServer;
pub use crate::ecs::{Entity, Schedule, System, World};
pub use crate::input::{CursorPosition, Input, KeyCode, MouseButton};
pub use crate::math::{Mat4, Quat, Rect, Transform, Vec2, Vec3, Vec4};
pub use crate::render::{ClearColor, GpuContext};
#[cfg(feature = "render2d")]
pub use crate::animation::{
    AnimationClip, AnimationPlayer, AnimationPlugin, EaseFunction, SpriteSheet, Tween,
    TweenTarget, advance_tweens, animate_sprites,
};
#[cfg(feature = "render2d")]
pub use crate::render2d::{Camera2d, Color, FontHandle, Shape2d, ShapeKind2d, Sprite, Text, TextureHandle, create_texture_from_rgba, load_font, load_texture};
#[cfg(feature = "render3d")]
pub use crate::render3d::{
    AmbientLight, Camera3d, DirectionalLight, Material, Mesh3d, MeshHandle, PointLight,
    Shape3d, ShapeKind3d, TextureHandle3d,
    cube_mesh, cylinder_mesh, load_texture_3d, plane_mesh, sphere_mesh,
};
#[cfg(all(feature = "render2d", feature = "physics2d"))]
pub use crate::render2d::DebugColliders2d;
#[cfg(all(feature = "render3d", feature = "physics3d"))]
pub use crate::render3d::DebugColliders3d;
#[cfg(feature = "audio")]
pub use crate::audio::{
    AudioEngine, AudioError, AudioPlugin, AudioSource, SoundData, SoundHandle, audio_system,
};
pub use crate::time::Time;
#[cfg(feature = "physics2d")]
pub use crate::physics2d::{
    Collider2d, ColliderShape2d, Physics2dPlugin, PhysicsWorld2d, RigidBody2d, RigidBodyType2d,
    physics_step_2d,
};
#[cfg(feature = "physics3d")]
pub use crate::physics3d::{
    Collider3d, ColliderShape3d, Physics3dPlugin, PhysicsWorld3d, RigidBody3d, RigidBodyType3d,
    physics_step_3d,
};
#[cfg(feature = "diagnostics")]
pub use crate::diag::ComponentRegistry;
