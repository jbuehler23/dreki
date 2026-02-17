//! # Query — Iterating Over Entities by Component Type
//!
//! Queries are how systems read and write component data. A query specifies
//! which component types it needs, and the ECS finds all entities that have
//! those components.
//!
//! ## How Queries Work
//!
//! ```text
//! world.query::<(&Position, &Velocity)>(|entity, (pos, vel)| {
//!     // use pos and vel
//! });
//!
//! 1. Compute TypeIds: [TypeId::of::<Position>(), TypeId::of::<Velocity>()]
//! 2. For each archetype in the world:
//!    - Does it contain BOTH Position AND Velocity? (superset check)
//!    - If yes, extract columns, iterate rows, restore columns
//! 3. Closure receives (Entity, (&Position, &Velocity)) per matching entity.
//! ```
//!
//! ## Closure-Based Design
//!
//! Rust's `Iterator` trait can't express "yielded items borrow from the
//! iterator" (lending iterators). Instead of using unsafe pointer casts, we
//! use a closure-based API with column extraction: columns are temporarily
//! removed from the archetype HashMap, giving us owned access that satisfies
//! the borrow checker, then restored afterward.
//!
//! ## The `QueryParam` Trait
//!
//! Rather than a single monolithic query type, we define a trait that any
//! "fetchable thing" implements. Tuples of query params are themselves query
//! params, so `(&A, &mut B)` just works.
//!
//! ## Comparison
//!
//! - **hecs**: Uses `Query` trait on tuples, very similar to our approach.
//! - **bevy_ecs**: Adds `FilteredAccess`, change detection, etc. Much more
//!   complex but same core idea.

use std::any::TypeId;
use std::collections::HashMap;

use super::component::ComponentColumn;

/// Trait for types that can be fetched from an archetype column.
///
/// Implemented for `&T` (shared read) and `&mut T` (exclusive write).
/// Tuple impls allow combining them: `(&A, &mut B, &C)`.
///
/// The `Column` associated type enables the extract/restore pattern: columns
/// are temporarily taken out of the archetype's HashMap so the borrow checker
/// can see that independent columns don't alias.
pub trait QueryParam {
    /// The item yielded per entity.
    type Item<'w>;

    /// Owned column data extracted from the archetype.
    type Column;

    /// The component TypeIds this parameter needs.
    fn type_ids() -> Vec<TypeId>;

    /// Extract the needed column(s) from the archetype's column map.
    fn extract(columns: &mut HashMap<TypeId, ComponentColumn>) -> Self::Column;

    /// Restore the column(s) back into the archetype's column map.
    fn restore(col: Self::Column, columns: &mut HashMap<TypeId, ComponentColumn>);

    /// Fetch the item for a single entity at `index` from the extracted column.
    fn fetch(col: &mut Self::Column, index: usize) -> Self::Item<'_>;
}

/// Shared read access to a component.
impl<T: 'static + Send + Sync> QueryParam for &T {
    type Item<'w> = &'w T;
    type Column = (TypeId, ComponentColumn);

    fn type_ids() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }

    fn extract(columns: &mut HashMap<TypeId, ComponentColumn>) -> Self::Column {
        let tid = TypeId::of::<T>();
        let col = columns.remove(&tid).unwrap_or_else(|| {
            panic!(
                "Query extract: column for `{}` not found in archetype",
                std::any::type_name::<T>()
            )
        });
        (tid, col)
    }

    fn restore(col: Self::Column, columns: &mut HashMap<TypeId, ComponentColumn>) {
        columns.insert(col.0, col.1);
    }

    fn fetch(col: &mut Self::Column, index: usize) -> Self::Item<'_> {
        col.1.get::<T>(index)
    }
}

/// Exclusive write access to a component.
impl<T: 'static + Send + Sync> QueryParam for &mut T {
    type Item<'w> = &'w mut T;
    type Column = (TypeId, ComponentColumn);

    fn type_ids() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }

    fn extract(columns: &mut HashMap<TypeId, ComponentColumn>) -> Self::Column {
        let tid = TypeId::of::<T>();
        let col = columns.remove(&tid).unwrap_or_else(|| {
            panic!(
                "Query extract: column for `{}` not found in archetype",
                std::any::type_name::<T>()
            )
        });
        (tid, col)
    }

    fn restore(col: Self::Column, columns: &mut HashMap<TypeId, ComponentColumn>) {
        columns.insert(col.0, col.1);
    }

    fn fetch(col: &mut Self::Column, index: usize) -> Self::Item<'_> {
        col.1.get_mut::<T>(index)
    }
}

/// Implement `QueryParam` for tuples of params.
///
/// This lets you write `world.query::<(&A, &mut B)>(|e, (a, b)| { ... })`
/// and get `(Entity, (&A, &mut B))` per matching entity.
macro_rules! impl_query_param_tuple {
    ($($P:ident),+) => {
        impl<$($P: QueryParam),+> QueryParam for ($($P,)+) {
            type Item<'w> = ($($P::Item<'w>,)+);
            type Column = ($($P::Column,)+);

            fn type_ids() -> Vec<TypeId> {
                let mut ids = Vec::new();
                $(ids.extend($P::type_ids());)+
                ids
            }

            #[allow(non_snake_case)]
            fn extract(columns: &mut HashMap<TypeId, ComponentColumn>) -> Self::Column {
                ($($P::extract(columns),)+)
            }

            #[allow(non_snake_case)]
            fn restore(col: Self::Column, columns: &mut HashMap<TypeId, ComponentColumn>) {
                let ($($P,)+) = col;
                $($P::restore($P, columns);)+
            }

            #[allow(non_snake_case)]
            fn fetch(col: &mut Self::Column, index: usize) -> Self::Item<'_> {
                let ($($P,)+) = col;
                ($($P::fetch($P, index),)+)
            }
        }
    };
}

impl_query_param_tuple!(A);
impl_query_param_tuple!(A, B);
impl_query_param_tuple!(A, B, C);
impl_query_param_tuple!(A, B, C, D);
impl_query_param_tuple!(A, B, C, D, E);
impl_query_param_tuple!(A, B, C, D, E, F);
impl_query_param_tuple!(A, B, C, D, E, F, G);
impl_query_param_tuple!(A, B, C, D, E, F, G, H);
