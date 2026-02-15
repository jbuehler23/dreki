//! # Custom Archetype-Based ECS
//!
//! This is a deliberately simple Entity Component System built as a learning
//! exercise. The design follows the archetype pattern used by
//! [hecs](https://github.com/Ralith/hecs) and
//! [bevy_ecs](https://github.com/bevyengine/bevy), but stripped down to the
//! essentials.
//!
//! ## Module Overview
//!
//! - [`entity`] — Generational entity IDs
//! - [`component`] — Type-erased columnar storage (`Box<dyn Any>`)
//! - [`archetype`] — Groups entities by component signature
//! - [`world`] — Central container (entities + components + resources)
//! - [`query`] — Closure-based iteration over matching archetypes
//! - [`system`] — System trait and schedule runner

pub(crate) mod archetype;
pub(crate) mod component;
pub mod entity;
pub mod hierarchy;
pub(crate) mod query;
pub mod system;
pub mod world;

pub use entity::Entity;
pub use hierarchy::{Children, GlobalTransform, HierarchyPlugin, Parent};
pub use system::{Schedule, System};
pub use world::World;
