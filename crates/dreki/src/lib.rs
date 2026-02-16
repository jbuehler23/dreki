//! # Dreki — Minimal Game Framework
//!
//! A lightweight game framework for rapid prototyping, built with a custom ECS,
//! wgpu rendering, and hot-reloadable assets.
//!
//! Start with `use dreki::prelude::*` and build a [`Game`](game::Game).

pub mod asset;
pub mod context;
pub mod ecs;
pub mod game;
pub mod input;
pub mod math;
pub mod prelude;
pub mod render;
pub mod scene;
pub mod time;
pub(crate) mod window;

#[cfg(feature = "render2d")]
pub mod animation;
#[cfg(feature = "render2d")]
pub mod render2d;

#[cfg(feature = "render3d")]
pub mod render3d;

#[cfg(feature = "audio")]
pub mod audio;

#[cfg(feature = "physics2d")]
pub mod physics2d;

#[cfg(feature = "physics3d")]
pub mod physics3d;

#[cfg(feature = "diagnostics")]
pub mod diag;
