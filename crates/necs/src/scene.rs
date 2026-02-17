//! # Scene Management — Save, Load, and Switch Scenes
//!
//! Provides serialization/deserialization of entities to JSON, a component
//! registry for type-erased serialize/deserialize, and scene switching via
//! entity tagging.
//!
//! ## Quick Start
//!
//! ```ignore
//! use necs::prelude::*;
//!
//! let mut registry = SceneRegistry::new();
//! registry.register::<Transform>();
//! registry.register::<MyComponent>();
//!
//! // Save all entities to JSON.
//! let data = save_scene(&world, &registry);
//! save_scene_to_file(&world, &registry, "level.json");
//!
//! // Load entities from JSON.
//! let entities = load_scene(&mut world, &registry, &data);
//! let entities = load_scene_from_file(&mut world, &registry, "level.json");
//! ```

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ecs::hierarchy::{Children, GlobalTransform, Parent};
use crate::ecs::world::World;
use crate::ecs::Entity;

// ── SceneRegistry ────────────────────────────────────────────────────────

type SerializeFn = fn(&dyn Any) -> Option<serde_json::Value>;
type DeserializeFn = fn(serde_json::Value) -> Option<Box<dyn Any + Send + Sync>>;

struct ComponentFns {
    serialize: SerializeFn,
    deserialize: DeserializeFn,
    default_fn: Option<Box<dyn Fn() -> serde_json::Value>>,
    short_name: String,
}

/// Maps component types to serialize/deserialize function pointers.
///
/// Register each component type you want to include in saved scenes.
pub struct SceneRegistry {
    by_type_id: HashMap<TypeId, ComponentFns>,
    by_name: HashMap<String, TypeId>,
}

impl SceneRegistry {
    pub fn new() -> Self {
        Self {
            by_type_id: HashMap::new(),
            by_name: HashMap::new(),
        }
    }

    /// Register a component type for scene serialization.
    ///
    /// If `T: Default`, a default value will be available for editor support.
    pub fn register<T>(&mut self)
    where
        T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        let full_name = std::any::type_name::<T>();
        let short = short_type_name(full_name);

        let fns = ComponentFns {
            serialize: |any| {
                let val = any.downcast_ref::<T>()?;
                serde_json::to_value(val).ok()
            },
            deserialize: |json| {
                let val: T = serde_json::from_value(json).ok()?;
                Some(Box::new(val))
            },
            default_fn: None,
            short_name: short.clone(),
        };

        self.by_type_id.insert(type_id, fns);
        self.by_name.insert(short, type_id);
    }

    /// Register a component with a default value (for types that implement Default).
    pub fn register_with_default<T>(&mut self, default: T)
    where
        T: Serialize + for<'de> Deserialize<'de> + Send + Sync + Clone + 'static,
    {
        let type_id = TypeId::of::<T>();
        let full_name = std::any::type_name::<T>();
        let short = short_type_name(full_name);

        let fns = ComponentFns {
            serialize: |any| {
                let val = any.downcast_ref::<T>()?;
                serde_json::to_value(val).ok()
            },
            deserialize: |json| {
                let val: T = serde_json::from_value(json).ok()?;
                Some(Box::new(val))
            },
            default_fn: Some(Box::new({
                let default = default.clone();
                move || serde_json::to_value(&default).unwrap_or(serde_json::Value::Null)
            })),
            short_name: short.clone(),
        };

        self.by_type_id.insert(type_id, fns);
        self.by_name.insert(short, type_id);
    }

    /// Returns all registered component names (for "Add Component" dropdown).
    pub fn component_names(&self) -> Vec<&str> {
        self.by_name.keys().map(|s| s.as_str()).collect()
    }

    /// Returns a default JSON value for a component type (for "Add Component" action).
    pub fn default_value(&self, name: &str) -> Option<serde_json::Value> {
        let type_id = self.by_name.get(name)?;
        let fns = self.by_type_id.get(type_id)?;
        let default_fn = fns.default_fn.as_ref()?;
        Some(default_fn())
    }

    // ── Convenience methods (wrap the free functions) ────────────────

    /// Save all entities in the world to a [`SceneData`].
    pub fn save(&self, world: &World) -> SceneData {
        save_scene(world, self)
    }

    /// Save all entities to a JSON file.
    pub fn save_to_file(&self, world: &World, path: impl AsRef<Path>) {
        save_scene_to_file(world, self, path)
    }

    /// Load entities from a [`SceneData`] into the world.
    pub fn load(&self, world: &mut World, data: &SceneData) -> Vec<Entity> {
        load_scene(world, self, data)
    }

    /// Load entities from a JSON file.
    pub fn load_from_file(&self, world: &mut World, path: impl AsRef<Path>) -> Vec<Entity> {
        load_scene_from_file(world, self, path)
    }

    /// Load entities with a scene tag for later cleanup.
    pub fn load_tagged(
        &self,
        world: &mut World,
        data: &SceneData,
        scene_name: &str,
    ) -> Vec<Entity> {
        load_scene_tagged(world, self, data, scene_name)
    }

    /// Despawn all entities tagged with a given scene name.
    pub fn unload(&self, world: &mut World, scene_name: &str) {
        unload_scene(world, scene_name)
    }

    /// Unload an old scene and load a new one.
    pub fn switch(
        &self,
        world: &mut World,
        old_name: &str,
        new_data: &SceneData,
        new_name: &str,
    ) -> Vec<Entity> {
        switch_scene(world, self, old_name, new_data, new_name)
    }
}

// ── Scene Data (JSON wire format) ────────────────────────────────────────

/// A serialized scene containing entities and their components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneData {
    pub entities: Vec<SceneEntity>,
}

/// A single entity in a serialized scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneEntity {
    pub id: u32,
    pub components: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<u32>,
}

// ── Save / Load functions ────────────────────────────────────────────────

/// Save all entities in the world to a [`SceneData`].
///
/// Hierarchy relationships are encoded in `SceneEntity.children` rather than
/// as components. `GlobalTransform`, `Parent`, and `Children` are not serialized.
pub fn save_scene(world: &World, registry: &SceneRegistry) -> SceneData {
    // First pass: collect all entities and their serialized components.
    let mut entity_map: HashMap<u32, SceneEntity> = HashMap::new();
    let skip_types = [
        TypeId::of::<Parent>(),
        TypeId::of::<Children>(),
        TypeId::of::<GlobalTransform>(),
        TypeId::of::<SceneMarker>(),
    ];

    world.for_each_entity(|entity, type_ids| {
        let mut components = HashMap::new();

        for &tid in type_ids {
            if skip_types.contains(&tid) {
                continue;
            }
            if let Some(fns) = registry.by_type_id.get(&tid) {
                if let Some(any) = world.get_any_by_type_id(entity, tid) {
                    if let Some(json) = (fns.serialize)(any) {
                        components.insert(fns.short_name.clone(), json);
                    }
                }
            }
        }

        entity_map.insert(
            entity.index(),
            SceneEntity {
                id: entity.index(),
                components,
                children: Vec::new(),
            },
        );
    });

    // Second pass: populate children lists from hierarchy.
    world.for_each_entity(|entity, type_ids| {
        let children_tid = TypeId::of::<Children>();
        if type_ids.contains(&children_tid) {
            if let Some(children) = world.get::<Children>(entity) {
                let child_ids: Vec<u32> = children
                    .0
                    .iter()
                    .filter(|&&c| world.is_alive(c))
                    .map(|c| c.index())
                    .collect();
                if let Some(scene_entity) = entity_map.get_mut(&entity.index()) {
                    scene_entity.children = child_ids;
                }
            }
        }
    });

    // Collect entities, with roots (no parent) first.
    let parent_tid = TypeId::of::<Parent>();
    let mut roots = Vec::new();
    let mut children_entities = Vec::new();

    world.for_each_entity(|entity, type_ids| {
        if type_ids.contains(&parent_tid) {
            children_entities.push(entity.index());
        } else {
            roots.push(entity.index());
        }
    });

    roots.sort();
    children_entities.sort();

    let mut entities = Vec::new();
    for id in roots.into_iter().chain(children_entities) {
        if let Some(scene_entity) = entity_map.remove(&id) {
            entities.push(scene_entity);
        }
    }

    SceneData { entities }
}

/// Load entities from a [`SceneData`] into the world.
///
/// Returns the list of spawned entities.
pub fn load_scene(
    world: &mut World,
    registry: &SceneRegistry,
    data: &SceneData,
) -> Vec<Entity> {
    // Map from scene entity ID → spawned Entity.
    let mut id_map: HashMap<u32, Entity> = HashMap::new();

    // First pass: spawn all entities with their components.
    for scene_entity in &data.entities {
        let entity = world.spawn_empty();
        id_map.insert(scene_entity.id, entity);

        for (name, json) in &scene_entity.components {
            if let Some(&type_id) = registry.by_name.get(name) {
                if let Some(fns) = registry.by_type_id.get(&type_id) {
                    if let Some(boxed) = (fns.deserialize)(json.clone()) {
                        insert_any(world, entity, type_id, name, boxed);
                    }
                }
            }
        }
    }

    // Second pass: reconstruct hierarchy from children arrays.
    for scene_entity in &data.entities {
        if scene_entity.children.is_empty() {
            continue;
        }
        let Some(&parent_entity) = id_map.get(&scene_entity.id) else {
            continue;
        };

        let mut child_entities = Vec::new();
        for &child_id in &scene_entity.children {
            if let Some(&child_entity) = id_map.get(&child_id) {
                child_entities.push(child_entity);
                world.insert(child_entity, Parent(parent_entity));
                world.insert(child_entity, GlobalTransform::default());
            }
        }

        if !child_entities.is_empty() {
            world.insert(parent_entity, Children(child_entities));
        }
    }

    id_map.values().copied().collect()
}

/// Save all entities to a JSON file.
pub fn save_scene_to_file(world: &World, registry: &SceneRegistry, path: impl AsRef<Path>) {
    let data = save_scene(world, registry);
    let json = serde_json::to_string_pretty(&data).expect("Failed to serialize scene");
    std::fs::write(path, json).expect("Failed to write scene file");
}

/// Load entities from a JSON file.
pub fn load_scene_from_file(
    world: &mut World,
    registry: &SceneRegistry,
    path: impl AsRef<Path>,
) -> Vec<Entity> {
    let json = std::fs::read_to_string(path).expect("Failed to read scene file");
    let data: SceneData = serde_json::from_str(&json).expect("Failed to deserialize scene");
    load_scene(world, registry, &data)
}

// ── Phase 3: Scene Switching ─────────────────────────────────────────────

/// Tags an entity as belonging to a named scene.
///
/// Used by [`load_scene_tagged`] and [`unload_scene`] for scene switching.
#[derive(Debug, Clone)]
pub struct SceneMarker(pub String);

/// Load entities from scene data and tag them all with a scene name.
pub fn load_scene_tagged(
    world: &mut World,
    registry: &SceneRegistry,
    data: &SceneData,
    scene_name: &str,
) -> Vec<Entity> {
    let entities = load_scene(world, registry, data);
    for &entity in &entities {
        world.insert(entity, SceneMarker(scene_name.to_string()));
    }
    entities
}

/// Despawn all entities tagged with a given scene name.
pub fn unload_scene(world: &mut World, scene_name: &str) {
    let mut to_despawn = Vec::new();
    world.query::<(&SceneMarker,)>(|entity, (marker,)| {
        if marker.0 == scene_name {
            to_despawn.push(entity);
        }
    });

    for entity in to_despawn {
        world.despawn_recursive(entity);
    }
}

/// Unload an old scene and load a new one.
///
/// Returns the entities spawned by the new scene.
pub fn switch_scene(
    world: &mut World,
    registry: &SceneRegistry,
    old_name: &str,
    new_data: &SceneData,
    new_name: &str,
) -> Vec<Entity> {
    unload_scene(world, old_name);
    load_scene_tagged(world, registry, new_data, new_name)
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Insert a type-erased component onto an entity.
fn insert_any(
    world: &mut World,
    entity: Entity,
    type_id: TypeId,
    name: &str,
    boxed: Box<dyn Any + Send + Sync>,
) {
    // Leak the name to get a 'static str. Component type names are finite and
    // small, so this is fine for the lifetime of the process.
    let static_name: &'static str = Box::leak(name.to_string().into_boxed_str());
    world.insert_any_component(entity, type_id, static_name, boxed);
}

fn short_type_name(full: &str) -> String {
    // Handle generic types like "alloc::vec::Vec<foo::Bar>" → "Vec<Bar>"
    // For now, just take the last segment before any `<`.
    if let Some(angle) = full.find('<') {
        let prefix = &full[..angle];
        let short_prefix = prefix.rsplit("::").next().unwrap_or(prefix);
        let inner = &full[angle + 1..full.len() - 1];
        let short_inner = short_type_name(inner);
        format!("{}<{}>", short_prefix, short_inner)
    } else {
        full.rsplit("::").next().unwrap_or(full).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Transform;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
    struct Health(u32);

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
    struct Name(String);

    fn test_registry() -> SceneRegistry {
        let mut registry = SceneRegistry::new();
        registry.register::<Transform>();
        registry.register::<Health>();
        registry.register::<Name>();
        registry
    }

    #[test]
    fn round_trip_simple() {
        let registry = test_registry();
        let mut world = World::new();

        world.spawn((
            Transform::from_xyz(1.0, 2.0, 3.0),
            Health(100),
        ));
        world.spawn((
            Transform::from_xy(10.0, 20.0),
            Name("hero".into()),
        ));

        let data = save_scene(&world, &registry);
        assert_eq!(data.entities.len(), 2);

        // Clear world and reload.
        world.despawn_all();
        assert_eq!(world.entity_count(), 0);

        let loaded = load_scene(&mut world, &registry, &data);
        assert_eq!(loaded.len(), 2);
        assert_eq!(world.entity_count(), 2);

        // Verify components round-tripped.
        let mut found_health = false;
        let mut found_name = false;
        world.query::<(&Health,)>(|_, (h,)| {
            assert_eq!(h.0, 100);
            found_health = true;
        });
        world.query::<(&Name,)>(|_, (n,)| {
            assert_eq!(n.0, "hero");
            found_name = true;
        });
        assert!(found_health);
        assert!(found_name);
    }

    #[test]
    fn round_trip_with_hierarchy() {
        let registry = test_registry();
        let mut world = World::new();

        let parent = world.spawn((Transform::from_xyz(100.0, 0.0, 0.0),));
        let _child = world.spawn_child(parent, (Transform::from_xyz(10.0, 0.0, 0.0),));

        let data = save_scene(&world, &registry);

        // Children should be in the scene data.
        let parent_entry = data.entities.iter().find(|e| e.children.len() == 1);
        assert!(parent_entry.is_some());

        // Clear and reload.
        world.despawn_all();
        let loaded = load_scene(&mut world, &registry, &data);
        assert_eq!(loaded.len(), 2);

        // Verify hierarchy was reconstructed.
        let mut parent_count = 0;
        let mut child_count = 0;
        world.query::<(&Children,)>(|_, _| parent_count += 1);
        world.query::<(&Parent,)>(|_, _| child_count += 1);
        assert_eq!(parent_count, 1);
        assert_eq!(child_count, 1);
    }

    #[test]
    fn scene_tagging_and_unload() {
        let registry = test_registry();
        let mut world = World::new();

        let data = SceneData {
            entities: vec![
                SceneEntity {
                    id: 0,
                    components: {
                        let mut m = HashMap::new();
                        m.insert("Health".into(), serde_json::json!(42));
                        m
                    },
                    children: vec![],
                },
                SceneEntity {
                    id: 1,
                    components: {
                        let mut m = HashMap::new();
                        m.insert("Health".into(), serde_json::json!(99));
                        m
                    },
                    children: vec![],
                },
            ],
        };

        let tagged = load_scene_tagged(&mut world, &registry, &data, "menu");
        assert_eq!(tagged.len(), 2);
        assert_eq!(world.entity_count(), 2);

        // Spawn a non-scene entity.
        world.spawn((Health(1),));
        assert_eq!(world.entity_count(), 3);

        // Unload the "menu" scene — only tagged entities removed.
        unload_scene(&mut world, "menu");
        assert_eq!(world.entity_count(), 1);
    }

    #[test]
    fn switch_scene_works() {
        let registry = test_registry();
        let mut world = World::new();

        let scene_a = SceneData {
            entities: vec![SceneEntity {
                id: 0,
                components: {
                    let mut m = HashMap::new();
                    m.insert("Health".into(), serde_json::json!(10));
                    m
                },
                children: vec![],
            }],
        };
        let scene_b = SceneData {
            entities: vec![
                SceneEntity {
                    id: 0,
                    components: {
                        let mut m = HashMap::new();
                        m.insert("Health".into(), serde_json::json!(50));
                        m
                    },
                    children: vec![],
                },
                SceneEntity {
                    id: 1,
                    components: {
                        let mut m = HashMap::new();
                        m.insert("Health".into(), serde_json::json!(60));
                        m
                    },
                    children: vec![],
                },
            ],
        };

        load_scene_tagged(&mut world, &registry, &scene_a, "a");
        assert_eq!(world.entity_count(), 1);

        let new_entities = switch_scene(&mut world, &registry, "a", &scene_b, "b");
        assert_eq!(new_entities.len(), 2);
        assert_eq!(world.entity_count(), 2);
    }

    #[test]
    fn component_names_and_defaults() {
        let mut registry = SceneRegistry::new();
        registry.register_with_default(Health(100));
        registry.register::<Name>();

        let names = registry.component_names();
        assert!(names.contains(&"Health"));
        assert!(names.contains(&"Name"));

        let default = registry.default_value("Health").unwrap();
        assert_eq!(default, serde_json::json!(100));

        assert!(registry.default_value("Name").is_none());
        assert!(registry.default_value("Nonexistent").is_none());
    }
}
