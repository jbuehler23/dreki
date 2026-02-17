//! # Component — Type-Erased Columnar Storage
//!
//! In an ECS, components are plain data — a `Position`, a `Velocity`, a
//! `Health`. The framework needs to store *any* component type without knowing
//! it at compile time (since archetypes are assembled dynamically). This module
//! provides [`ComponentColumn`], a type-erased column of components.
//!
//! ## Why `Box<dyn Any>`?
//!
//! Archetypes contain a *dynamic* set of component types. We can't use
//! `Vec<T>` because the archetype doesn't know `T` — it only knows a
//! [`TypeId`]. The classic approach (used by hecs, bevy_ecs) stores raw bytes
//! (`Vec<u8>`) with manual layout management — fast but requires `unsafe`.
//!
//! We use `Vec<Box<dyn Any + Send + Sync>>` instead. Each component is heap-
//! allocated and accessed via `downcast_ref`/`downcast_mut`. This trades cache
//! locality for **zero unsafe code** — the right tradeoff for a learning
//! framework where clarity matters more than nanoseconds.
//!
//! ## Comparison
//!
//! - **hecs / bevy_ecs**: `Vec<u8>` + `Layout` (BlobVec). Cache-friendly,
//!   lots of unsafe.
//! - **necs**: `Vec<Box<dyn Any>>`. Zero unsafe, simple, easy to audit.

use std::any::{Any, TypeId};

/// Returns the `TypeId` for a component type. Components must be `'static +
/// Send + Sync` — this is enforced at the call sites (spawn, query).
pub(crate) fn component_type_id<T: 'static>() -> TypeId {
    TypeId::of::<T>()
}

/// A type-erased column of components, stored as boxed trait objects.
///
/// This is the core storage primitive. Each [`Archetype`](super::archetype::Archetype)
/// has one `ComponentColumn` per component type.
///
/// All access is safe — type correctness is ensured via `downcast_ref`/`downcast_mut`
/// at runtime, with panics on mismatch (which indicates a framework bug).
/// Opaque type-erased column used internally by queries and spawn bundles.
/// Users interact with components through [`World`](super::world::World) methods.
pub struct ComponentColumn {
    data: Vec<Box<dyn Any + Send + Sync>>,
}

impl ComponentColumn {
    /// Create a new empty column.
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// Push a typed component onto the end of the column.
    pub fn push<T: 'static + Send + Sync>(&mut self, value: T) {
        self.data.push(Box::new(value));
    }

    /// Get a shared reference to the component at `index`.
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds or the type doesn't match.
    pub fn get<T: 'static>(&self, index: usize) -> &T {
        self.data[index]
            .downcast_ref()
            .unwrap_or_else(|| {
                panic!(
                    "Component type mismatch: expected `{}` in column",
                    std::any::type_name::<T>()
                )
            })
    }

    /// Get a mutable reference to the component at `index`.
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds or the type doesn't match.
    pub fn get_mut<T: 'static>(&mut self, index: usize) -> &mut T {
        self.data[index]
            .downcast_mut()
            .unwrap_or_else(|| {
                panic!(
                    "Component type mismatch: expected `{}` in column",
                    std::any::type_name::<T>()
                )
            })
    }

    /// Swap-remove the component at `index`, returning whether a swap occurred.
    ///
    /// Returns `true` if the removed element wasn't the last one (i.e., the
    /// last element was swapped into the removed slot).
    pub fn swap_remove(&mut self, index: usize) -> bool {
        let last = self.data.len() - 1;
        let swapped = index != last;
        self.data.swap_remove(index);
        swapped
    }

    /// Remove the component at `index` via swap-remove and return it as a
    /// boxed `Any`. Used when moving components between archetypes.
    pub fn take(&mut self, index: usize) -> Box<dyn Any + Send + Sync> {
        self.data.swap_remove(index)
    }

    /// Push a type-erased boxed component. Used when moving components between
    /// archetypes.
    pub fn push_any(&mut self, value: Box<dyn Any + Send + Sync>) {
        self.data.push(value);
    }

    /// Get a reference to the raw `dyn Any` at `index`.
    pub fn get_any(&self, index: usize) -> &dyn Any {
        &*self.data[index]
    }

    /// Number of components stored.
    pub fn len(&self) -> usize {
        self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_get() {
        let mut col = ComponentColumn::new();
        col.push(1.0f32);
        col.push(2.0f32);
        col.push(3.0f32);
        assert_eq!(*col.get::<f32>(0), 1.0);
        assert_eq!(*col.get::<f32>(1), 2.0);
        assert_eq!(*col.get::<f32>(2), 3.0);
        assert_eq!(col.len(), 3);
    }

    #[test]
    fn swap_remove_middle() {
        let mut col = ComponentColumn::new();
        col.push(10u32);
        col.push(20u32);
        col.push(30u32);
        let swapped = col.swap_remove(0);
        assert!(swapped); // last element (30) moved to index 0
        assert_eq!(col.len(), 2);
        assert_eq!(*col.get::<u32>(0), 30);
        assert_eq!(*col.get::<u32>(1), 20);
    }

    #[test]
    fn swap_remove_last() {
        let mut col = ComponentColumn::new();
        col.push(10u32);
        col.push(20u32);
        let swapped = col.swap_remove(1);
        assert!(!swapped); // removed the last element, no swap
        assert_eq!(col.len(), 1);
        assert_eq!(*col.get::<u32>(0), 10);
    }

    #[test]
    fn drop_called_on_remove() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

        struct Tracked;
        impl Drop for Tracked {
            fn drop(&mut self) {
                DROP_COUNT.fetch_add(1, Ordering::SeqCst);
            }
        }

        DROP_COUNT.store(0, Ordering::SeqCst);
        let mut col = ComponentColumn::new();
        col.push(Tracked);
        col.push(Tracked);
        col.swap_remove(0);
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1); // only the removed one
        drop(col);
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2); // remaining one dropped
    }

    #[test]
    fn zst_components() {
        struct Marker;
        let mut col = ComponentColumn::new();
        col.push(Marker);
        col.push(Marker);
        assert_eq!(col.len(), 2);
    }

    #[test]
    fn take_and_push_any() {
        let mut col = ComponentColumn::new();
        col.push(42u64);
        col.push(99u64);

        let taken = col.take(0);
        assert_eq!(col.len(), 1);
        assert_eq!(*col.get::<u64>(0), 99);

        let mut col2 = ComponentColumn::new();
        col2.push_any(taken);
        assert_eq!(*col2.get::<u64>(0), 42);
    }
}
