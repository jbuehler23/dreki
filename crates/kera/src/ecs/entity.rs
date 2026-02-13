//! # Entity — Lightweight Identifiers for Game Objects
//!
//! An [`Entity`] is just a number — it doesn't "contain" anything. Instead, the
//! [`World`](super::world::World) maps entities to their components. This
//! separation of identity from data is the core insight of the ECS pattern.
//!
//! ## Design: Generational Indices
//!
//! A naive approach would use an incrementing counter for entity IDs, but this
//! breaks when entities are destroyed and their IDs recycled. Consider:
//!
//! ```text
//! 1. Spawn entity #5
//! 2. Store a reference: saved = Entity(5)
//! 3. Despawn entity #5
//! 4. Spawn a new entity — gets recycled ID #5
//! 5. Use `saved` — oops, it now refers to the wrong entity!
//! ```
//!
//! The fix: pair each index with a **generation** counter. When a slot is
//! recycled, its generation increments. Any stale handle with an old generation
//! is detected as invalid.
//!
//! ```text
//! Entity { index: 5, generation: 0 }  ← original
//! Entity { index: 5, generation: 1 }  ← after recycle
//! ```
//!
//! Stale handle still says `generation: 0`, so lookups fail safely.
//!
//! ## Comparison
//!
//! - **hecs**: Uses `Entity` = u64 split into index + generation (same idea).
//! - **bevy_ecs**: Same generational index scheme, but wraps it in more layers.
//! - **EnTT (C++)**: Uses a similar packed integer approach.
//!
//! Our implementation is intentionally minimal: two `u32` fields, no bit
//! packing, easy to understand.

use std::fmt;

/// A lightweight handle to an entity in the [`World`](super::world::World).
///
/// Entities are created via [`World::spawn`] and destroyed via
/// [`World::despawn`]. An `Entity` is only valid for the `World` that created
/// it, and only while its generation matches.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Entity {
    /// Slot index in the allocator. This is recycled when the entity is despawned.
    pub(crate) index: u32,
    /// Generation counter. Incremented each time this slot is reused, so stale
    /// handles can be detected.
    pub(crate) generation: u32,
}

impl Entity {
    /// Returns the raw index. Useful for diagnostics, not for general use.
    pub fn index(self) -> u32 {
        self.index
    }

    /// Returns the generation. Useful for diagnostics.
    pub fn generation(self) -> u32 {
        self.generation
    }
}

impl fmt::Debug for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Entity({}v{})", self.index, self.generation)
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}v{}", self.index, self.generation)
    }
}

/// Manages entity ID allocation and recycling.
///
/// ## Memory Layout
///
/// ```text
/// generations: [0, 1, 0, 2, 0]   ← one generation per slot ever allocated
/// free_list:   [1, 3]             ← slots available for reuse
/// len:         5                   ← next fresh index (if free_list is empty)
/// ```
///
/// When spawning: pop from `free_list` if available, otherwise use `len` and grow.
/// When despawning: increment generation, push index onto `free_list`.
pub(crate) struct EntityAllocator {
    /// Generation counter for each slot. Index into this with `Entity::index`.
    generations: Vec<u32>,
    /// Indices of despawned entities, available for reuse.
    free_list: Vec<u32>,
    /// Total number of slots ever allocated. Also the next fresh index.
    len: u32,
}

impl EntityAllocator {
    pub fn new() -> Self {
        Self {
            generations: Vec::new(),
            free_list: Vec::new(),
            len: 0,
        }
    }

    /// Allocate a new [`Entity`]. Reuses a freed slot if one is available,
    /// otherwise allocates a fresh index.
    pub fn allocate(&mut self) -> Entity {
        if let Some(index) = self.free_list.pop() {
            // Reuse a recycled slot — generation was already bumped on dealloc.
            let generation = self.generations[index as usize];
            Entity { index, generation }
        } else {
            // Fresh slot.
            let index = self.len;
            self.len += 1;
            self.generations.push(0);
            Entity {
                index,
                generation: 0,
            }
        }
    }

    /// Deallocate an entity, making its slot available for reuse.
    ///
    /// Returns `true` if the entity was valid and successfully deallocated,
    /// `false` if it was already stale.
    pub fn deallocate(&mut self, entity: Entity) -> bool {
        let idx = entity.index as usize;
        if idx < self.generations.len() && self.generations[idx] == entity.generation {
            // Bump generation so any existing handles become stale.
            self.generations[idx] += 1;
            self.free_list.push(entity.index);
            true
        } else {
            false
        }
    }

    /// Check if an entity handle is still valid (not despawned or stale).
    pub fn is_alive(&self, entity: Entity) -> bool {
        let idx = entity.index as usize;
        idx < self.generations.len() && self.generations[idx] == entity.generation
    }

    /// Returns the number of currently alive entities.
    pub fn alive_count(&self) -> usize {
        (self.len as usize) - self.free_list.len()
    }

    /// Returns the number of free (recyclable) slots.
    #[cfg(any(feature = "diagnostics", test))]
    pub(crate) fn free_count(&self) -> usize {
        self.free_list.len()
    }

    /// Returns the total number of slots ever allocated.
    #[cfg(any(feature = "diagnostics", test))]
    pub(crate) fn total_slots(&self) -> u32 {
        self.len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_sequential() {
        let mut alloc = EntityAllocator::new();
        let e0 = alloc.allocate();
        let e1 = alloc.allocate();
        assert_eq!(e0.index, 0);
        assert_eq!(e1.index, 1);
        assert_eq!(e0.generation, 0);
        assert_eq!(e1.generation, 0);
    }

    #[test]
    fn recycle_bumps_generation() {
        let mut alloc = EntityAllocator::new();
        let e0 = alloc.allocate();
        assert!(alloc.deallocate(e0));
        let e0_reused = alloc.allocate();
        assert_eq!(e0_reused.index, 0); // same slot
        assert_eq!(e0_reused.generation, 1); // bumped
    }

    #[test]
    fn stale_handle_detected() {
        let mut alloc = EntityAllocator::new();
        let e0 = alloc.allocate();
        assert!(alloc.is_alive(e0));
        alloc.deallocate(e0);
        assert!(!alloc.is_alive(e0)); // stale
    }

    #[test]
    fn double_free_returns_false() {
        let mut alloc = EntityAllocator::new();
        let e0 = alloc.allocate();
        assert!(alloc.deallocate(e0));
        assert!(!alloc.deallocate(e0)); // already freed
    }

    #[test]
    fn alive_count() {
        let mut alloc = EntityAllocator::new();
        assert_eq!(alloc.alive_count(), 0);
        let e0 = alloc.allocate();
        let _e1 = alloc.allocate();
        assert_eq!(alloc.alive_count(), 2);
        alloc.deallocate(e0);
        assert_eq!(alloc.alive_count(), 1);
    }

    #[test]
    fn free_count_and_total_slots() {
        let mut alloc = EntityAllocator::new();
        assert_eq!(alloc.free_count(), 0);
        assert_eq!(alloc.total_slots(), 0);

        let e0 = alloc.allocate();
        let _e1 = alloc.allocate();
        assert_eq!(alloc.total_slots(), 2);
        assert_eq!(alloc.free_count(), 0);

        alloc.deallocate(e0);
        assert_eq!(alloc.total_slots(), 2);
        assert_eq!(alloc.free_count(), 1);
    }
}
