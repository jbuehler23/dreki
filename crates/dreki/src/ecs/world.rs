//! # World — The Central Container
//!
//! The [`World`] owns all entities, components, and resources. It's the single
//! source of truth for the entire game state.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │ World                                               │
//! │                                                     │
//! │  EntityAllocator: manages entity ID lifecycle        │
//! │                                                     │
//! │  archetypes: HashMap<ArchetypeKey, Archetype>       │
//! │    key = sorted Vec<TypeId>                         │
//! │    value = Archetype { columns, entities }           │
//! │                                                     │
//! │  entity_locations: HashMap<u32, EntityLocation>     │
//! │    maps entity index → (archetype key, row index)   │
//! │                                                     │
//! │  resources: HashMap<TypeId, Box<dyn Any>>           │
//! │    singleton data not tied to an entity              │
//! └─────────────────────────────────────────────────────┘
//! ```
//!
//! ## Resources
//!
//! Resources are "global" data — things like the `Time`, `Input` state, or
//! `AssetServer`. They're stored as type-erased `Box<dyn Any>` in a HashMap.
//! This is simpler than making them entities with special components.
//!
//! ## Comparison
//!
//! - **hecs**: World stores only entities/components. No built-in resources.
//! - **bevy_ecs**: World has entities, components, resources, schedules,
//!   observers, hooks... much more.
//!
//! We include resources because they're essential for a usable framework, but
//! keep everything else minimal.

use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet};

use super::archetype::{Archetype, ArchetypeKey, archetype_key};
use super::component::{ComponentColumn, component_type_id};
use super::entity::{Entity, EntityAllocator};
use super::query::QueryParam;

/// Location of an entity within the archetype storage.
#[derive(Clone)]
pub(crate) struct EntityLocation {
    /// Which archetype this entity lives in.
    archetype_key: ArchetypeKey,
    /// Row index within that archetype.
    row: usize,
}

/// The central container for all game state.
///
/// Owns all entities, their components (organized into archetypes), and
/// global resources.
pub struct World {
    allocator: EntityAllocator,
    /// All archetypes, keyed by their sorted component type set.
    archetypes: HashMap<ArchetypeKey, Archetype>,
    /// Maps entity index → its location in archetype storage.
    entity_locations: HashMap<u32, EntityLocation>,
    /// Global resources (singletons), keyed by TypeId.
    resources: HashMap<TypeId, Box<dyn Any>>,
    /// Named entity lookup: name → entity.
    names: HashMap<String, Entity>,
    /// Reverse lookup: entity index → name.
    names_reverse: HashMap<u32, String>,
    /// Tag → set of entities with that tag.
    tags: HashMap<String, HashSet<Entity>>,
    /// Entity index → tags on that entity.
    entity_tags: HashMap<u32, Vec<String>>,
    /// Number of entities spawned this frame (diagnostics only).
    #[cfg(feature = "diagnostics")]
    spawned_this_frame: u32,
    /// Number of entities despawned this frame (diagnostics only).
    #[cfg(feature = "diagnostics")]
    despawned_this_frame: u32,
}

impl World {
    pub fn new() -> Self {
        Self {
            allocator: EntityAllocator::new(),
            archetypes: HashMap::new(),
            entity_locations: HashMap::new(),
            resources: HashMap::new(),
            names: HashMap::new(),
            names_reverse: HashMap::new(),
            tags: HashMap::new(),
            entity_tags: HashMap::new(),
            #[cfg(feature = "diagnostics")]
            spawned_this_frame: 0,
            #[cfg(feature = "diagnostics")]
            despawned_this_frame: 0,
        }
    }

    // ── Resources ────────────────────────────────────────────────────

    /// Insert a resource (singleton value). Replaces any existing resource of
    /// the same type.
    pub fn insert_resource<T: 'static + Send + Sync>(&mut self, value: T) {
        self.resources.insert(TypeId::of::<T>(), Box::new(value));
    }

    /// Get a shared reference to a resource.
    ///
    /// # Panics
    ///
    /// Panics if the resource hasn't been inserted.
    pub fn resource<T: 'static + Send + Sync>(&self) -> &T {
        self.resources
            .get(&TypeId::of::<T>())
            .unwrap_or_else(|| {
                panic!(
                    "Resource `{}` not found. Did you forget to insert it?",
                    std::any::type_name::<T>()
                )
            })
            .downcast_ref::<T>()
            .unwrap()
    }

    /// Get a mutable reference to a resource.
    ///
    /// # Panics
    ///
    /// Panics if the resource hasn't been inserted.
    pub fn resource_mut<T: 'static + Send + Sync>(&mut self) -> &mut T {
        self.resources
            .get_mut(&TypeId::of::<T>())
            .unwrap_or_else(|| {
                panic!(
                    "Resource `{}` not found. Did you forget to insert it?",
                    std::any::type_name::<T>()
                )
            })
            .downcast_mut::<T>()
            .unwrap()
    }

    /// Try to get a shared reference to a resource. Returns `None` if not found.
    pub fn get_resource<T: 'static + Send + Sync>(&self) -> Option<&T> {
        self.resources
            .get(&TypeId::of::<T>())
            .and_then(|r| r.downcast_ref::<T>())
    }

    /// Try to get a mutable reference to a resource. Returns `None` if not found.
    pub fn get_resource_mut<T: 'static + Send + Sync>(&mut self) -> Option<&mut T> {
        self.resources
            .get_mut(&TypeId::of::<T>())
            .and_then(|r| r.downcast_mut::<T>())
    }

    /// Check if a resource exists.
    pub fn has_resource<T: 'static + Send + Sync>(&self) -> bool {
        self.resources.contains_key(&TypeId::of::<T>())
    }

    /// Remove a resource, taking ownership. Returns `None` if not present.
    ///
    /// Use this for the extract/reinsert pattern when you need to borrow a
    /// resource while also borrowing the world (e.g., during rendering).
    pub fn resource_remove<T: 'static + Send + Sync>(&mut self) -> Option<T> {
        self.resources
            .remove(&TypeId::of::<T>())
            .and_then(|r| r.downcast::<T>().ok())
            .map(|b| *b)
    }

    /// Check if any non-empty archetype contains a component of type `T`.
    pub(crate) fn has_component_type<T: 'static + Send + Sync>(&self) -> bool {
        let type_id = TypeId::of::<T>();
        self.archetypes
            .values()
            .any(|a| a.has_component(&type_id) && !a.entities.is_empty())
    }

    // ── Named Entities ─────────────────────────────────────────────

    /// Get the entity with the given name.
    ///
    /// # Panics
    ///
    /// Panics if no entity has that name.
    pub fn named(&self, name: &str) -> Entity {
        *self.names.get(name).unwrap_or_else(|| {
            panic!("No entity named \"{}\"", name)
        })
    }

    /// Try to get the entity with the given name. Returns `None` if not found.
    pub fn try_named(&self, name: &str) -> Option<Entity> {
        self.names.get(name).copied()
    }

    /// Assign a name to an entity. Used internally by Context::spawn().
    ///
    /// # Panics
    ///
    /// Panics if the name is already in use.
    pub(crate) fn name_entity(&mut self, entity: Entity, name: &str) {
        if let Some(&existing) = self.names.get(name) {
            panic!(
                "Name \"{}\" is already used by entity {:?} (tried to assign to {:?})",
                name, existing, entity
            );
        }
        self.names.insert(name.to_string(), entity);
        self.names_reverse.insert(entity.index(), name.to_string());
    }

    // ── Tags ──────────────────────────────────────────────────────────

    /// Add a tag to an entity. An entity can have multiple tags,
    /// and many entities can share the same tag.
    pub fn tag(&mut self, entity: Entity, tag: &str) {
        self.tags
            .entry(tag.to_string())
            .or_insert_with(HashSet::new)
            .insert(entity);
        self.entity_tags
            .entry(entity.index())
            .or_insert_with(Vec::new)
            .push(tag.to_string());
    }

    /// Get all entities with a given tag.
    pub fn tagged(&self, tag: &str) -> Vec<Entity> {
        self.tags
            .get(tag)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Collect all entities that have a component of type `T`.
    pub fn entities_with<T: 'static + Send + Sync>(&self) -> Vec<Entity> {
        let type_id = TypeId::of::<T>();
        let mut result = Vec::new();
        for arch in self.archetypes.values() {
            if arch.has_component(&type_id) {
                result.extend_from_slice(&arch.entities);
            }
        }
        result
    }

    // ── Entity Management ────────────────────────────────────────────

    /// Returns the number of alive entities.
    pub fn entity_count(&self) -> usize {
        self.allocator.alive_count()
    }

    /// Returns the number of archetypes.
    pub fn archetype_count(&self) -> usize {
        self.archetypes.len()
    }

    /// Check if an entity is alive.
    pub fn is_alive(&self, entity: Entity) -> bool {
        self.allocator.is_alive(entity)
    }

    /// Collect a diagnostics snapshot of ECS state.
    ///
    /// Returns (entity_count, archetype_count, archetype_infos).
    /// Entity details are included for archetypes whose index appears in
    /// `expanded_archetypes`. The `registry` is used to format component values.
    #[cfg(feature = "diagnostics")]
    pub(crate) fn diagnostics_snapshot(
        &self,
        expanded_archetypes: &[usize],
        registry: Option<&crate::diag::ComponentRegistry>,
    ) -> (usize, usize, Vec<crate::diag::ArchetypeSnapshot>) {
        let entity_count = self.allocator.alive_count();
        let archetype_count = self.archetypes.len();

        // Collect archetype keys in a deterministic order.
        let mut keys: Vec<_> = self.archetypes.keys().collect();
        keys.sort_by(|a, b| {
            // Sort by entity count descending, then by key for stability.
            let arch_a = &self.archetypes[*a];
            let arch_b = &self.archetypes[*b];
            arch_b.entities.len().cmp(&arch_a.entities.len())
        });

        let mut archetypes = Vec::with_capacity(keys.len());
        for (idx, key) in keys.iter().enumerate() {
            let arch = &self.archetypes[*key];
            // Skip empty archetypes.
            if arch.entities.is_empty() {
                continue;
            }

            let component_names: Vec<String> = key
                .iter()
                .map(|tid| {
                    arch.type_name_map
                        .get(tid)
                        .map(|n| short_type_name(n))
                        .unwrap_or_else(|| format!("{:?}", tid))
                })
                .collect();

            // Include entity details for expanded archetypes.
            let entities = if expanded_archetypes.contains(&idx) {
                let mut entity_infos = Vec::with_capacity(arch.entities.len());
                for (row, &entity) in arch.entities.iter().enumerate() {
                    let mut components = Vec::new();
                    for tid in key.iter() {
                        let name = arch
                            .type_name_map
                            .get(tid)
                            .map(|n| short_type_name(n))
                            .unwrap_or_else(|| format!("{:?}", tid));
                        let debug_value = if let (Some(reg), Some(col)) =
                            (&registry, arch.columns.get(tid))
                        {
                            reg.format(tid, col.get_any(row))
                        } else {
                            "<opaque>".to_string()
                        };
                        components.push(crate::diag::ComponentSnapshot {
                            name,
                            debug_value,
                        });
                    }
                    // Hierarchy info for this entity.
                    let parent_id = self
                        .get::<crate::ecs::hierarchy::Parent>(entity)
                        .map(|p| p.0.index());
                    let child_count = self
                        .get::<crate::ecs::hierarchy::Children>(entity)
                        .map(|c| c.0.len() as u32)
                        .unwrap_or(0);

                    entity_infos.push(crate::diag::EntitySnapshot {
                        id: entity.index(),
                        generation: entity.generation(),
                        components,
                        parent_id,
                        child_count,
                    });
                }
                Some(entity_infos)
            } else {
                None
            };

            archetypes.push(crate::diag::ArchetypeSnapshot {
                entity_count: arch.entities.len(),
                component_names,
                entities,
            });
        }

        (entity_count, archetype_count, archetypes)
    }

    /// Collect entity pool statistics and reset per-frame counters.
    #[cfg(feature = "diagnostics")]
    pub(crate) fn diagnostics_entity_stats(&mut self) -> crate::diag::EntityPoolStats {
        let total_slots = self.allocator.total_slots();
        let free_count = self.allocator.free_count();
        let alive = self.allocator.alive_count();
        let spawned = self.spawned_this_frame;
        let despawned = self.despawned_this_frame;
        self.spawned_this_frame = 0;
        self.despawned_this_frame = 0;
        crate::diag::EntityPoolStats {
            total_slots,
            free_count,
            alive_count: alive,
            spawned_this_tick: spawned,
            despawned_this_tick: despawned,
        }
    }

    // ── Spawn / Despawn ──────────────────────────────────────────────

    /// Spawn an entity with no components.
    pub fn spawn_empty(&mut self) -> Entity {
        let entity = self.allocator.allocate();
        #[cfg(feature = "diagnostics")]
        { self.spawned_this_frame += 1; }
        let key = archetype_key(vec![]);
        self.archetypes
            .entry(key.clone())
            .or_insert_with(|| Archetype::new(HashMap::new()));
        let arch = self.archetypes.get_mut(&key).unwrap();
        let row = arch.entities.len();
        arch.entities.push(entity);
        self.entity_locations.insert(
            entity.index,
            EntityLocation {
                archetype_key: key,
                row,
            },
        );
        entity
    }

    /// Spawn a child entity under a parent. Adds [`Parent`] and
    /// [`GlobalTransform`] to the child, and updates the parent's [`Children`].
    ///
    /// # Panics
    ///
    /// Panics if the parent entity is not alive.
    pub fn spawn_child<B: SpawnBundle>(&mut self, parent: Entity, bundle: B) -> Entity {
        use crate::ecs::hierarchy::{Children, GlobalTransform, Parent};

        assert!(
            self.allocator.is_alive(parent),
            "Cannot spawn child on dead parent {:?}",
            parent
        );

        let child = self.spawn(bundle);
        self.insert(child, Parent(parent));
        self.insert(child, GlobalTransform::default());

        // Update parent's Children list.
        if let Some(children) = self.get_mut::<Children>(parent) {
            children.0.push(child);
        } else {
            self.insert(parent, Children(vec![child]));
        }

        child
    }

    /// Despawn an entity and all its descendants recursively.
    ///
    /// Also removes the entity from its parent's [`Children`] list if it has one.
    ///
    /// Returns `true` if the entity was alive and successfully despawned.
    pub fn despawn_recursive(&mut self, entity: Entity) -> bool {
        use crate::ecs::hierarchy::{Children, Parent};

        if !self.allocator.is_alive(entity) {
            return false;
        }

        // Remove from parent's Children list.
        if let Some(parent_entity) = self.get::<Parent>(entity).map(|p| p.0) {
            if let Some(children) = self.get_mut::<Children>(parent_entity) {
                children.0.retain(|&c| c != entity);
            }
        }

        // Collect all descendants via BFS.
        let mut to_despawn = vec![entity];
        let mut i = 0;
        while i < to_despawn.len() {
            let current = to_despawn[i];
            if let Some(children) = self.get::<Children>(current) {
                let child_list: Vec<_> = children.0.clone();
                to_despawn.extend(child_list);
            }
            i += 1;
        }

        // Despawn all collected entities.
        for e in to_despawn {
            self.despawn(e);
        }

        true
    }

    /// Despawn every entity in the world.
    pub fn despawn_all(&mut self) {
        // Collect all alive entities.
        let mut all_entities = Vec::new();
        for arch in self.archetypes.values() {
            for &entity in &arch.entities {
                all_entities.push(entity);
            }
        }
        for entity in all_entities {
            self.despawn(entity);
        }
        // Clear name/tag maps (despawn already removes per-entity, but this
        // ensures a clean slate even if something was missed).
        self.names.clear();
        self.names_reverse.clear();
        self.tags.clear();
        self.entity_tags.clear();
    }

    /// Despawn an entity, removing it from its archetype and freeing its ID
    /// for reuse.
    ///
    /// Returns `true` if the entity was alive and successfully despawned.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.allocator.is_alive(entity) {
            return false;
        }

        // Clean up name.
        if let Some(name) = self.names_reverse.remove(&entity.index()) {
            self.names.remove(&name);
        }

        // Clean up tags.
        if let Some(tags) = self.entity_tags.remove(&entity.index()) {
            for tag in tags {
                if let Some(set) = self.tags.get_mut(&tag) {
                    set.remove(&entity);
                    if set.is_empty() {
                        self.tags.remove(&tag);
                    }
                }
            }
        }

        if let Some(loc) = self.entity_locations.remove(&entity.index) {
            if let Some(arch) = self.archetypes.get_mut(&loc.archetype_key) {
                let swapped = arch.swap_remove(loc.row);

                // If an entity was swapped into the removed slot, update its location.
                if let Some(swapped_entity) = swapped {
                    if let Some(swapped_loc) =
                        self.entity_locations.get_mut(&swapped_entity.index)
                    {
                        swapped_loc.row = loc.row;
                    }
                }
            }
        }

        self.allocator.deallocate(entity);
        #[cfg(feature = "diagnostics")]
        { self.despawned_this_frame += 1; }
        true
    }

    // ── Per-Entity Component Access ──────────────────────────────────

    /// Get a shared reference to a component on a specific entity.
    ///
    /// Returns `None` if the entity is dead or doesn't have the component.
    pub fn get<T: 'static + Send + Sync>(&self, entity: Entity) -> Option<&T> {
        if !self.allocator.is_alive(entity) {
            return None;
        }
        let loc = self.entity_locations.get(&entity.index)?;
        let arch = self.archetypes.get(&loc.archetype_key)?;
        let col = arch.columns.get(&TypeId::of::<T>())?;
        Some(col.get::<T>(loc.row))
    }

    /// Get a mutable reference to a component on a specific entity.
    ///
    /// Returns `None` if the entity is dead or doesn't have the component.
    pub fn get_mut<T: 'static + Send + Sync>(&mut self, entity: Entity) -> Option<&mut T> {
        if !self.allocator.is_alive(entity) {
            return None;
        }
        let loc = self.entity_locations.get(&entity.index)?;
        let arch = self.archetypes.get_mut(&loc.archetype_key)?;
        let col = arch.columns.get_mut(&TypeId::of::<T>())?;
        Some(col.get_mut::<T>(loc.row))
    }

    // ── Dynamic Component Add/Remove ─────────────────────────────────

    /// Add a component to an existing entity, moving it to a new archetype.
    ///
    /// If the entity already has a component of this type, it is replaced.
    ///
    /// # Panics
    ///
    /// Panics if the entity is not alive.
    pub fn insert<T: 'static + Send + Sync>(&mut self, entity: Entity, component: T) {
        assert!(
            self.allocator.is_alive(entity),
            "Cannot insert component `{}` on dead entity {:?}",
            std::any::type_name::<T>(),
            entity
        );

        let loc = self.entity_locations.get(&entity.index).unwrap().clone();
        let tid = TypeId::of::<T>();

        // If the entity already has this component type, just replace it.
        if let Some(arch) = self.archetypes.get_mut(&loc.archetype_key) {
            if arch.columns.contains_key(&tid) {
                *arch.columns.get_mut(&tid).unwrap().get_mut::<T>(loc.row) = component;
                return;
            }
        }

        // Build the new archetype key.
        let mut new_type_ids = loc.archetype_key.clone();
        new_type_ids.push(tid);
        let new_key = archetype_key(new_type_ids);

        // Ensure the target archetype exists.
        if !self.archetypes.contains_key(&new_key) {
            let mut columns = HashMap::new();
            for &t in &new_key {
                columns.insert(t, ComponentColumn::new());
            }
            self.archetypes
                .insert(new_key.clone(), Archetype::new(columns));
        }

        // Take all components from the old archetype for this entity.
        let old_arch = self.archetypes.get_mut(&loc.archetype_key).unwrap();
        let mut taken: HashMap<TypeId, Box<dyn Any + Send + Sync>> = HashMap::new();
        for (&col_tid, col) in old_arch.columns.iter_mut() {
            taken.insert(col_tid, col.take(loc.row));
        }
        // Remove entity from old archetype's entity list.
        old_arch.entities.swap_remove(loc.row);
        // Update the swapped entity's location if needed.
        if loc.row < old_arch.entities.len() {
            let swapped_entity = old_arch.entities[loc.row];
            if let Some(swapped_loc) = self.entity_locations.get_mut(&swapped_entity.index) {
                swapped_loc.row = loc.row;
            }
        }

        // Push into new archetype: existing components from taken, new component directly.
        let new_arch = self.archetypes.get_mut(&new_key).unwrap();
        let new_row = new_arch.entities.len();
        new_arch.entities.push(entity);
        // Push the new component.
        new_arch
            .columns
            .get_mut(&tid)
            .unwrap()
            .push::<T>(component);
        // Push all existing components.
        for (&col_tid, col) in new_arch.columns.iter_mut() {
            if col_tid != tid {
                col.push_any(taken.remove(&col_tid).unwrap());
            }
        }

        self.entity_locations.insert(
            entity.index,
            EntityLocation {
                archetype_key: new_key,
                row: new_row,
            },
        );
    }

    /// Remove a component from an existing entity, moving it to a new archetype.
    ///
    /// Returns `true` if the component was present and removed, `false` otherwise.
    ///
    /// # Panics
    ///
    /// Panics if the entity is not alive.
    pub fn remove<T: 'static + Send + Sync>(&mut self, entity: Entity) -> bool {
        assert!(
            self.allocator.is_alive(entity),
            "Cannot remove component `{}` from dead entity {:?}",
            std::any::type_name::<T>(),
            entity
        );

        let loc = self.entity_locations.get(&entity.index).unwrap().clone();
        let tid = TypeId::of::<T>();

        // Check if entity actually has this component.
        if let Some(arch) = self.archetypes.get(&loc.archetype_key) {
            if !arch.columns.contains_key(&tid) {
                return false;
            }
        } else {
            return false;
        }

        // Build the new archetype key (without this type).
        let new_key: ArchetypeKey = loc
            .archetype_key
            .iter()
            .copied()
            .filter(|&t| t != tid)
            .collect();

        // Ensure the target archetype exists.
        if !self.archetypes.contains_key(&new_key) {
            let mut columns = HashMap::new();
            for &t in &new_key {
                columns.insert(t, ComponentColumn::new());
            }
            self.archetypes
                .insert(new_key.clone(), Archetype::new(columns));
        }

        // Take all components from the old archetype for this entity.
        let old_arch = self.archetypes.get_mut(&loc.archetype_key).unwrap();
        let mut taken: HashMap<TypeId, Box<dyn Any + Send + Sync>> = HashMap::new();
        for (&col_tid, col) in old_arch.columns.iter_mut() {
            taken.insert(col_tid, col.take(loc.row));
        }
        // Remove entity from old archetype's entity list.
        old_arch.entities.swap_remove(loc.row);
        // Update the swapped entity's location if needed.
        if loc.row < old_arch.entities.len() {
            let swapped_entity = old_arch.entities[loc.row];
            if let Some(swapped_loc) = self.entity_locations.get_mut(&swapped_entity.index) {
                swapped_loc.row = loc.row;
            }
        }

        // Push into new archetype (skipping the removed component).
        let new_arch = self.archetypes.get_mut(&new_key).unwrap();
        let new_row = new_arch.entities.len();
        new_arch.entities.push(entity);
        for (&col_tid, col) in new_arch.columns.iter_mut() {
            col.push_any(taken.remove(&col_tid).unwrap());
        }
        // The removed component's Box is in `taken` and will be dropped here.

        self.entity_locations.insert(
            entity.index,
            EntityLocation {
                archetype_key: new_key,
                row: new_row,
            },
        );

        true
    }

    // ── Type-Erased Component Insertion (for scene deserialization) ──

    /// Insert a type-erased component onto an entity, migrating it to a new
    /// archetype. Used by the scene loader to insert deserialized components
    /// without knowing the concrete type at compile time.
    pub(crate) fn insert_any_component(
        &mut self,
        entity: Entity,
        type_id: TypeId,
        type_name: &'static str,
        boxed: Box<dyn Any + Send + Sync>,
    ) {
        assert!(
            self.allocator.is_alive(entity),
            "Cannot insert component on dead entity {:?}",
            entity
        );

        let loc = self.entity_locations.get(&entity.index).unwrap().clone();

        // If the entity already has this component type, skip (shouldn't happen
        // in normal scene load).
        if let Some(arch) = self.archetypes.get(&loc.archetype_key) {
            if arch.columns.contains_key(&type_id) {
                return;
            }
        }

        // Build new archetype key.
        let mut new_type_ids = loc.archetype_key.clone();
        new_type_ids.push(type_id);
        let new_key = archetype_key(new_type_ids);

        // Ensure target archetype exists.
        if !self.archetypes.contains_key(&new_key) {
            let mut columns = HashMap::new();
            for &t in &new_key {
                columns.insert(t, ComponentColumn::new());
            }
            self.archetypes
                .insert(new_key.clone(), Archetype::new(columns));
        }

        // Take all components from old archetype.
        let old_arch = self.archetypes.get_mut(&loc.archetype_key).unwrap();
        let mut taken: HashMap<TypeId, Box<dyn Any + Send + Sync>> = HashMap::new();
        for (&col_tid, col) in old_arch.columns.iter_mut() {
            taken.insert(col_tid, col.take(loc.row));
        }
        old_arch.entities.swap_remove(loc.row);
        if loc.row < old_arch.entities.len() {
            let swapped_entity = old_arch.entities[loc.row];
            if let Some(swapped_loc) = self.entity_locations.get_mut(&swapped_entity.index) {
                swapped_loc.row = loc.row;
            }
        }

        // Push into new archetype.
        let new_arch = self.archetypes.get_mut(&new_key).unwrap();
        let new_row = new_arch.entities.len();
        new_arch.entities.push(entity);
        new_arch.type_name_map.entry(type_id).or_insert(type_name);

        // Push the new component.
        new_arch
            .columns
            .get_mut(&type_id)
            .unwrap()
            .push_any(boxed);
        // Push existing components.
        for (&col_tid, col) in new_arch.columns.iter_mut() {
            if col_tid != type_id {
                if let Some(val) = taken.remove(&col_tid) {
                    col.push_any(val);
                }
            }
        }

        self.entity_locations.insert(
            entity.index,
            EntityLocation {
                archetype_key: new_key,
                row: new_row,
            },
        );
    }

    // ── Type-Erased Access (for scene serialization) ────────────────

    /// Iterate all alive entities, calling `f` with the entity and its component TypeIds.
    pub fn for_each_entity(&self, mut f: impl FnMut(Entity, &[TypeId])) {
        for (key, arch) in &self.archetypes {
            for &entity in &arch.entities {
                f(entity, key);
            }
        }
    }

    /// Get a type-erased reference to a component by TypeId.
    ///
    /// Returns `None` if the entity doesn't have that component type.
    pub fn get_any_by_type_id(&self, entity: Entity, type_id: TypeId) -> Option<&dyn Any> {
        if !self.allocator.is_alive(entity) {
            return None;
        }
        let loc = self.entity_locations.get(&entity.index)?;
        let arch = self.archetypes.get(&loc.archetype_key)?;
        let col = arch.columns.get(&type_id)?;
        Some(col.get_any(loc.row))
    }

    // ── Query ────────────────────────────────────────────────────────

    /// Query all entities that have the requested component types.
    ///
    /// Takes a closure that receives `(Entity, Q::Item)` for each matching
    /// entity across all archetypes.
    ///
    /// # Example
    ///
    /// ```ignore
    /// world.query::<(&mut Position, &Velocity)>(|entity, (pos, vel)| {
    ///     pos.x += vel.dx;
    /// });
    /// ```
    pub fn query<Q: QueryParam>(&mut self, mut f: impl FnMut(Entity, Q::Item<'_>)) {
        let required_types = Q::type_ids();

        // Collect matching archetype keys first to avoid borrow issues.
        let matching_keys: Vec<ArchetypeKey> = self
            .archetypes
            .iter()
            .filter(|(_, arch)| required_types.iter().all(|tid| arch.has_component(tid)))
            .map(|(key, _)| key.clone())
            .collect();

        for key in matching_keys {
            let arch = self.archetypes.get_mut(&key).unwrap();
            let mut cols = Q::extract(&mut arch.columns);
            let entity_count = arch.entities.len();
            for i in 0..entity_count {
                let entity = arch.entities[i];
                f(entity, Q::fetch(&mut cols, i));
            }
            Q::restore(cols, &mut arch.columns);
        }
    }

    /// Query with an additional filter: only entities that also have a marker
    /// component `F`.
    ///
    /// The filter component is not yielded — it's just a presence check.
    ///
    /// # Example
    ///
    /// ```ignore
    /// world.query_filtered::<(&mut Transform,), Player>(|entity, (transform,)| {
    ///     transform.translation.x += speed;
    /// });
    /// ```
    pub fn query_filtered<Q: QueryParam, F: 'static + Send + Sync>(
        &mut self,
        mut f: impl FnMut(Entity, Q::Item<'_>),
    ) {
        let mut required_types = Q::type_ids();
        required_types.push(TypeId::of::<F>());

        let matching_keys: Vec<ArchetypeKey> = self
            .archetypes
            .iter()
            .filter(|(_, arch)| required_types.iter().all(|tid| arch.has_component(tid)))
            .map(|(key, _)| key.clone())
            .collect();

        for key in matching_keys {
            let arch = self.archetypes.get_mut(&key).unwrap();
            let mut cols = Q::extract(&mut arch.columns);
            let entity_count = arch.entities.len();
            for i in 0..entity_count {
                let entity = arch.entities[i];
                f(entity, Q::fetch(&mut cols, i));
            }
            Q::restore(cols, &mut arch.columns);
        }
    }

    /// Query for a single entity that has the requested components and a
    /// marker component `F`.
    ///
    /// Calls the closure with `(Entity, Q::Item)` if exactly one entity
    /// matches. Does nothing if no entity matches. Panics if more than one
    /// entity matches (singleton invariant violated).
    ///
    /// # Example
    ///
    /// ```ignore
    /// world.query_single::<(&Camera,), MainCamera>(|entity, (cam,)| {
    ///     // use cam
    /// });
    /// ```
    pub fn query_single<Q: QueryParam, F: 'static + Send + Sync>(
        &mut self,
        f: impl FnOnce(Entity, Q::Item<'_>),
    ) {
        let mut required_types = Q::type_ids();
        required_types.push(TypeId::of::<F>());

        let matching_keys: Vec<ArchetypeKey> = self
            .archetypes
            .iter()
            .filter(|(_, arch)| required_types.iter().all(|tid| arch.has_component(tid)))
            .map(|(key, _)| key.clone())
            .collect();

        // Find the single matching entity.
        let mut found: Option<(Entity, ArchetypeKey, usize)> = None;
        for key in &matching_keys {
            let arch = self.archetypes.get(key).unwrap();
            for i in 0..arch.entities.len() {
                if found.is_some() {
                    panic!(
                        "query_single: multiple entities match filter `{}`",
                        std::any::type_name::<F>()
                    );
                }
                found = Some((arch.entities[i], key.clone(), i));
            }
        }

        if let Some((entity, key, index)) = found {
            let arch = self.archetypes.get_mut(&key).unwrap();
            let mut cols = Q::extract(&mut arch.columns);
            f(entity, Q::fetch(&mut cols, index));
            Q::restore(cols, &mut arch.columns);
        }
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

// ── Spawn Trait (tuple support) ──────────────────────────────────────────

/// Trait for component bundles that can be spawned into the world.
///
/// Implemented for tuples of components up to 8 elements. Each component must
/// be `'static + Send + Sync`.
pub trait SpawnBundle {
    fn type_ids() -> Vec<TypeId>;
    /// Human-readable type names for each component type.
    fn type_names() -> Vec<(TypeId, &'static str)>;
    /// Create empty columns for each component type in the bundle.
    fn create_columns() -> HashMap<TypeId, ComponentColumn>;
    /// Push all components into the matching columns.
    fn push_into(self, columns: &mut HashMap<TypeId, ComponentColumn>);
}

macro_rules! impl_spawn_bundle {
    ($($T:ident),+) => {
        impl<$($T: 'static + Send + Sync),+> SpawnBundle for ($($T,)+) {
            fn type_ids() -> Vec<TypeId> {
                vec![$(component_type_id::<$T>()),+]
            }

            fn type_names() -> Vec<(TypeId, &'static str)> {
                vec![$((component_type_id::<$T>(), std::any::type_name::<$T>())),+]
            }

            fn create_columns() -> HashMap<TypeId, ComponentColumn> {
                let mut map = HashMap::new();
                $(map.insert(component_type_id::<$T>(), ComponentColumn::new());)+
                map
            }

            #[allow(non_snake_case)]
            fn push_into(self, columns: &mut HashMap<TypeId, ComponentColumn>) {
                let ($($T,)+) = self;
                $(
                    columns.get_mut(&component_type_id::<$T>()).unwrap().push::<$T>($T);
                )+
            }
        }
    };
}

impl_spawn_bundle!(A);
impl_spawn_bundle!(A, B);
impl_spawn_bundle!(A, B, C);
impl_spawn_bundle!(A, B, C, D);
impl_spawn_bundle!(A, B, C, D, E);
impl_spawn_bundle!(A, B, C, D, E, F);
impl_spawn_bundle!(A, B, C, D, E, F, G);
impl_spawn_bundle!(A, B, C, D, E, F, G, H);

impl World {
    /// Spawn an entity with a bundle of components (tuple).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let e = world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { dx: 1.0, dy: 0.0 }));
    /// ```
    pub fn spawn<B: SpawnBundle>(&mut self, bundle: B) -> Entity {
        let entity = self.allocator.allocate();
        #[cfg(feature = "diagnostics")]
        { self.spawned_this_frame += 1; }
        let key = archetype_key(B::type_ids());

        // Ensure the archetype exists.
        if !self.archetypes.contains_key(&key) {
            let columns = B::create_columns();
            self.archetypes
                .insert(key.clone(), Archetype::new(columns));
        }

        let arch = self.archetypes.get_mut(&key).unwrap();
        // Populate type names (idempotent — only meaningful on first spawn).
        for (tid, name) in B::type_names() {
            arch.type_name_map.entry(tid).or_insert(name);
        }
        let row = arch.entities.len();
        arch.entities.push(entity);
        bundle.push_into(&mut arch.columns);

        self.entity_locations.insert(
            entity.index,
            EntityLocation {
                archetype_key: key,
                row,
            },
        );

        entity
    }

    /// Spawn an entity with a single component — no tuple wrapping needed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let e = world.spawn_one(Marker);
    /// let e = world.spawn_one(Position { x: 0.0, y: 0.0 });
    /// ```
    pub fn spawn_one<T: 'static + Send + Sync>(&mut self, component: T) -> Entity {
        self.spawn((component,))
    }
}

/// Strip the module path from a fully-qualified type name, keeping only the
/// short name (e.g. `dreki::math::Transform` → `Transform`).
#[cfg(feature = "diagnostics")]
fn short_type_name(full: &str) -> String {
    full.rsplit("::").next().unwrap_or(full).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
    }
    #[derive(Debug, PartialEq)]
    struct Velocity {
        dx: f32,
        dy: f32,
    }
    struct Health(u32);
    struct Marker;
    struct Shield;
    struct Poisoned {
        damage: u32,
    }

    #[test]
    fn spawn_and_query() {
        let mut world = World::new();
        world.spawn((
            Position { x: 1.0, y: 2.0 },
            Velocity { dx: 0.5, dy: -0.5 },
        ));
        world.spawn((
            Position { x: 3.0, y: 4.0 },
            Velocity { dx: 1.0, dy: 1.0 },
        ));
        world.spawn((Position { x: 5.0, y: 6.0 },)); // no velocity

        let mut results = Vec::new();
        world.query::<(&Position, &Velocity)>(|_, (p, v)| {
            results.push((p.x, p.y, v.dx, v.dy));
        });

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn spawn_and_despawn() {
        let mut world = World::new();
        let e1 = world.spawn((Position { x: 0.0, y: 0.0 },));
        let e2 = world.spawn((Position { x: 1.0, y: 1.0 },));
        assert_eq!(world.entity_count(), 2);

        world.despawn(e1);
        assert_eq!(world.entity_count(), 1);
        assert!(!world.is_alive(e1));
        assert!(world.is_alive(e2));

        // Query should only return e2.
        let mut results = Vec::new();
        world.query::<(&Position,)>(|_, (p,)| {
            results.push(p.x);
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 1.0);
    }

    #[test]
    fn resources() {
        let mut world = World::new();
        world.insert_resource(42u32);
        world.insert_resource(String::from("hello"));

        assert_eq!(*world.resource::<u32>(), 42);
        assert_eq!(world.resource::<String>(), "hello");

        *world.resource_mut::<u32>() = 99;
        assert_eq!(*world.resource::<u32>(), 99);
    }

    #[test]
    fn resource_remove_and_reinsert() {
        let mut world = World::new();
        world.insert_resource(String::from("hello"));

        let taken = world.resource_remove::<String>();
        assert_eq!(taken, Some(String::from("hello")));
        assert!(!world.has_resource::<String>());

        // Reinsert
        world.insert_resource(taken.unwrap());
        assert_eq!(world.resource::<String>(), "hello");

        // Remove nonexistent
        assert_eq!(world.resource_remove::<u64>(), None);
    }

    #[test]
    fn query_mutate() {
        let mut world = World::new();
        world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { dx: 1.0, dy: 2.0 }));

        world.query::<(&mut Position, &Velocity)>(|_, (pos, vel)| {
            pos.x += vel.dx;
            pos.y += vel.dy;
        });

        let mut results = Vec::new();
        world.query::<(&Position,)>(|_, (p,)| {
            results.push((p.x, p.y));
        });
        assert_eq!(results[0], (1.0, 2.0));
    }

    #[test]
    fn query_filtered_with_marker() {
        let mut world = World::new();
        world.spawn((Position { x: 0.0, y: 0.0 }, Marker));
        world.spawn((Position { x: 1.0, y: 1.0 },)); // no marker

        let mut results = Vec::new();
        world.query_filtered::<(&Position,), Marker>(|_, (p,)| {
            results.push(p.x);
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 0.0);
    }

    #[test]
    fn despawn_swap_remove_preserves_data() {
        let mut world = World::new();
        let e0 = world.spawn((Health(10),));
        let _e1 = world.spawn((Health(20),));
        let _e2 = world.spawn((Health(30),));

        world.despawn(e0);

        // The remaining entities should have Health(30) and Health(20)
        // (30 was swapped into slot 0).
        let mut healths = Vec::new();
        world.query::<(&Health,)>(|_, (h,)| {
            healths.push(h.0);
        });
        healths.sort();
        assert_eq!(healths, vec![20, 30]);
    }

    // ── New API tests ────────────────────────────────────────────────

    #[test]
    fn spawn_one_component() {
        let mut world = World::new();
        let e = world.spawn_one(Marker);
        assert!(world.is_alive(e));
        assert_eq!(world.entity_count(), 1);
    }

    #[test]
    fn get_component() {
        let mut world = World::new();
        let e = world.spawn((Position { x: 42.0, y: 99.0 },));

        let pos = world.get::<Position>(e).unwrap();
        assert_eq!(pos.x, 42.0);
        assert_eq!(pos.y, 99.0);

        // Missing component returns None.
        assert!(world.get::<Velocity>(e).is_none());
    }

    #[test]
    fn get_mut_component() {
        let mut world = World::new();
        let e = world.spawn((Position { x: 0.0, y: 0.0 },));

        world.get_mut::<Position>(e).unwrap().x = 10.0;
        assert_eq!(world.get::<Position>(e).unwrap().x, 10.0);
    }

    #[test]
    fn get_dead_entity_returns_none() {
        let mut world = World::new();
        let e = world.spawn((Position { x: 0.0, y: 0.0 },));
        world.despawn(e);
        assert!(world.get::<Position>(e).is_none());
    }

    #[test]
    fn insert_new_component() {
        let mut world = World::new();
        let e = world.spawn((Position { x: 1.0, y: 2.0 },));

        // Entity doesn't have Velocity yet.
        assert!(world.get::<Velocity>(e).is_none());

        world.insert(e, Velocity { dx: 3.0, dy: 4.0 });

        // Now it does.
        let vel = world.get::<Velocity>(e).unwrap();
        assert_eq!(vel.dx, 3.0);
        assert_eq!(vel.dy, 4.0);

        // Original component still intact.
        let pos = world.get::<Position>(e).unwrap();
        assert_eq!(pos.x, 1.0);
        assert_eq!(pos.y, 2.0);
    }

    #[test]
    fn insert_replaces_existing_component() {
        let mut world = World::new();
        let e = world.spawn((Health(50),));

        world.insert(e, Health(100));
        assert_eq!(world.get::<Health>(e).unwrap().0, 100);
    }

    #[test]
    fn remove_component() {
        let mut world = World::new();
        let e = world.spawn((Position { x: 1.0, y: 2.0 }, Shield));

        assert!(world.remove::<Shield>(e));
        assert!(world.get::<Shield>(e).is_none());

        // Position still intact.
        let pos = world.get::<Position>(e).unwrap();
        assert_eq!(pos.x, 1.0);
    }

    #[test]
    fn remove_nonexistent_component_returns_false() {
        let mut world = World::new();
        let e = world.spawn((Position { x: 0.0, y: 0.0 },));
        assert!(!world.remove::<Shield>(e));
    }

    #[test]
    fn insert_and_query() {
        let mut world = World::new();
        let e = world.spawn((Position { x: 0.0, y: 0.0 },));
        world.insert(e, Poisoned { damage: 5 });

        let mut results = Vec::new();
        world.query::<(&Position, &Poisoned)>(|_, (pos, poison)| {
            results.push((pos.x, poison.damage));
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], (0.0, 5));
    }

    #[test]
    fn query_single_finds_singleton() {
        let mut world = World::new();
        world.spawn((Position { x: 42.0, y: 0.0 }, Marker));

        let mut found = false;
        world.query_single::<(&Position,), Marker>(|_, (pos,)| {
            assert_eq!(pos.x, 42.0);
            found = true;
        });
        assert!(found);
    }

    #[test]
    fn query_single_no_match() {
        let mut world = World::new();
        world.spawn((Position { x: 0.0, y: 0.0 },)); // no Marker

        let mut called = false;
        world.query_single::<(&Position,), Marker>(|_, _| {
            called = true;
        });
        assert!(!called);
    }

    #[test]
    #[should_panic(expected = "multiple entities")]
    fn query_single_panics_on_multiple() {
        let mut world = World::new();
        world.spawn((Position { x: 0.0, y: 0.0 }, Marker));
        world.spawn((Position { x: 1.0, y: 1.0 }, Marker));

        world.query_single::<(&Position,), Marker>(|_, _| {});
    }

    // ── Named entity tests ───────────────────────────────────────────

    #[test]
    fn named_entity_lookup() {
        let mut world = World::new();
        let e = world.spawn((Position { x: 1.0, y: 2.0 },));
        world.name_entity(e, "player");

        assert_eq!(world.named("player"), e);
        assert_eq!(world.try_named("player"), Some(e));
        assert_eq!(world.try_named("nonexistent"), None);
    }

    #[test]
    #[should_panic(expected = "No entity named")]
    fn named_panics_on_missing() {
        let world = World::new();
        world.named("ghost");
    }

    #[test]
    #[should_panic(expected = "already used")]
    fn duplicate_name_panics() {
        let mut world = World::new();
        let e1 = world.spawn((Position { x: 0.0, y: 0.0 },));
        let e2 = world.spawn((Position { x: 1.0, y: 1.0 },));
        world.name_entity(e1, "hero");
        world.name_entity(e2, "hero");
    }

    #[test]
    fn despawn_cleans_up_name() {
        let mut world = World::new();
        let e = world.spawn((Marker,));
        world.name_entity(e, "temp");
        assert!(world.try_named("temp").is_some());

        world.despawn(e);
        assert!(world.try_named("temp").is_none());
    }

    // ── Tag tests ────────────────────────────────────────────────────

    #[test]
    fn tag_and_query() {
        let mut world = World::new();
        let e1 = world.spawn((Position { x: 0.0, y: 0.0 },));
        let e2 = world.spawn((Position { x: 1.0, y: 1.0 },));
        let _e3 = world.spawn((Position { x: 2.0, y: 2.0 },));

        world.tag(e1, "enemy");
        world.tag(e2, "enemy");

        let enemies = world.tagged("enemy");
        assert_eq!(enemies.len(), 2);
        assert!(enemies.contains(&e1));
        assert!(enemies.contains(&e2));
    }

    #[test]
    fn tagged_returns_empty_for_unknown() {
        let world = World::new();
        assert!(world.tagged("nothing").is_empty());
    }

    #[test]
    fn despawn_cleans_up_tags() {
        let mut world = World::new();
        let e = world.spawn((Marker,));
        world.tag(e, "temporary");
        assert_eq!(world.tagged("temporary").len(), 1);

        world.despawn(e);
        assert!(world.tagged("temporary").is_empty());
    }

    #[test]
    fn despawn_all_cleans_names_and_tags() {
        let mut world = World::new();
        let e1 = world.spawn((Marker,));
        let e2 = world.spawn((Marker,));
        world.name_entity(e1, "a");
        world.name_entity(e2, "b");
        world.tag(e1, "group");
        world.tag(e2, "group");

        world.despawn_all();
        assert!(world.try_named("a").is_none());
        assert!(world.try_named("b").is_none());
        assert!(world.tagged("group").is_empty());
    }

    // ── entities_with tests ──────────────────────────────────────────

    #[test]
    fn entities_with_component() {
        let mut world = World::new();
        let e1 = world.spawn((Position { x: 0.0, y: 0.0 }, Marker));
        let _e2 = world.spawn((Position { x: 1.0, y: 1.0 },));
        let e3 = world.spawn((Marker,));

        let with_marker = world.entities_with::<Marker>();
        assert_eq!(with_marker.len(), 2);
        assert!(with_marker.contains(&e1));
        assert!(with_marker.contains(&e3));
    }
}
