//! # Archetype — Grouping Entities by Component Signature
//!
//! An archetype represents a unique combination of component types. All
//! entities that have exactly the same set of components are stored together in
//! the same archetype. This is the key optimization that makes ECS queries
//! fast.
//!
//! ## Why Archetypes?
//!
//! Consider two alternative designs:
//!
//! 1. **Sparse set per component** (EnTT-style): Each component type has its
//!    own array. Fast to add/remove components, but iterating over *pairs* of
//!    components requires intersecting two sparse sets.
//!
//! 2. **Archetype table** (hecs/bevy-style): Entities with the same component
//!    set are stored together. Iteration is a simple linear scan through
//!    matching archetypes — extremely cache-friendly.
//!
//! We chose archetype tables because:
//! - Iteration is the hot path in games (every frame, many systems scan entities).
//! - Adding/removing components is rarer and can be amortized.
//! - The data layout is easier to understand and debug.
//!
//! ## Memory Layout
//!
//! ```text
//! Archetype { type_ids: [Position, Velocity] }
//!
//! columns:
//!   Position: [pos0, pos1, pos2, pos3]    ← one Box<dyn Any> per component
//!   Velocity: [vel0, vel1, vel2, vel3]    ← one Box<dyn Any> per component
//! entities:   [e0,   e1,   e2,   e3  ]    ← parallel array
//!
//! All arrays have the same length. Index `i` in every column and in the
//! entity array refers to the same entity.
//! ```
//!
//! ## Comparison
//!
//! - **hecs**: Very similar archetype design. Stores columns in a `HashMap<TypeId, ...>`.
//! - **bevy_ecs**: Adds more metadata (table IDs, change ticks, etc.) but same concept.

use std::any::TypeId;
use std::collections::HashMap;

use super::component::ComponentColumn;
use super::entity::Entity;

/// A sorted list of [`TypeId`]s that uniquely identifies an archetype.
///
/// We sort so that `(A, B)` and `(B, A)` produce the same key.
pub(crate) type ArchetypeKey = Vec<TypeId>;

/// Compute the archetype key for a set of type IDs (sorted, deduplicated).
pub(crate) fn archetype_key(mut type_ids: Vec<TypeId>) -> ArchetypeKey {
    type_ids.sort();
    type_ids.dedup();
    type_ids
}

/// An archetype: a table of entities that all share the same component types.
pub(crate) struct Archetype {
    /// One column per component type, keyed by `TypeId`.
    pub columns: HashMap<TypeId, ComponentColumn>,
    /// Which entities live in this archetype, parallel to the column rows.
    pub entities: Vec<Entity>,
    /// Human-readable type names, populated during spawn.
    pub type_name_map: HashMap<TypeId, &'static str>,
}

impl Archetype {
    /// Create a new empty archetype with the given columns.
    pub fn new(columns: HashMap<TypeId, ComponentColumn>) -> Self {
        Self {
            columns,
            entities: Vec::new(),
            type_name_map: HashMap::new(),
        }
    }

    /// Check whether this archetype contains a given component type.
    pub fn has_component(&self, type_id: &TypeId) -> bool {
        self.columns.contains_key(type_id)
    }

    /// Swap-remove an entity at `index`. Returns the entity that was moved
    /// into the removed slot (if any — `None` if we removed the last one).
    pub fn swap_remove(&mut self, index: usize) -> Option<Entity> {
        for column in self.columns.values_mut() {
            column.swap_remove(index);
        }

        // Swap-remove from the entity list.
        self.entities.swap_remove(index);

        // If something was swapped in, return it so the caller can update
        // the entity-to-archetype index.
        if index < self.entities.len() {
            Some(self.entities[index])
        } else {
            None
        }
    }
}
