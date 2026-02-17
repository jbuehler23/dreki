//! Programmatic scene construction and lifecycle management.
//!
//! [`Template`] lets you define reusable entity blueprints with components and
//! children. [`SceneBuilder`] composes templates into named scenes with optional
//! enter/exit callbacks. [`SceneManager`] handles transitions between scenes.
//!
//! Use the [`Scenes`] plugin to register everything automatically.
//!
//! # Example
//!
//! ```ignore
//! use necs::prelude::*;
//!
//! let enemy = Template::new()
//!     .with(Transform::from_xy(100.0, 0.0))
//!     .with(Sprite::new().color(Color::RED).size(30.0, 30.0))
//!     .tag("enemy");
//!
//! let menu = SceneBuilder::new("menu")
//!     .add(Template::new()
//!         .with(Transform::from_xy(0.0, 100.0))
//!         .with(Sprite::new().color(Color::GREEN).size(200.0, 60.0)))
//!     .on_enter(|ctx| { log::info!("Entered menu"); });
//!
//! Game::new("My Game")
//!     .plugin(Scenes::new().add(menu).start("menu"))
//!     .run();
//! ```

use std::cell::RefCell;
use std::sync::Mutex;

use crate::context::Context;
use crate::ecs::hierarchy::{Children, GlobalTransform, Parent};
use crate::ecs::Entity;
use crate::ecs::world::World;
use crate::scene::SceneMarker;

/// Callback type for scene enter/exit hooks. Wrapped in Mutex for Sync.
type SceneCallback = Mutex<Box<dyn FnMut(&mut Context) + Send>>;

// ── ComponentAdder (type-erased component storage) ─────────────────────

/// Type-erased trait for cloning a component and inserting it into an entity.
trait ComponentAdder: Send + Sync {
    fn insert_into(&self, world: &mut World, entity: Entity);
    fn clone_box(&self) -> Box<dyn ComponentAdder>;
}

/// Concrete implementor of [`ComponentAdder`] for a specific type.
struct TypedAdder<T: Clone + Send + Sync + 'static> {
    value: T,
}

impl<T: Clone + Send + Sync + 'static> ComponentAdder for TypedAdder<T> {
    fn insert_into(&self, world: &mut World, entity: Entity) {
        world.insert(entity, self.value.clone());
    }

    fn clone_box(&self) -> Box<dyn ComponentAdder> {
        Box::new(TypedAdder {
            value: self.value.clone(),
        })
    }
}

// ── Template ───────────────────────────────────────────────────────────

/// A reusable entity blueprint. Stores components (type-erased) and children.
///
/// Components must be `Clone + Send + Sync + 'static`. Templates themselves are
/// cloneable and can be spawned multiple times.
///
/// # Example
///
/// ```ignore
/// let player = Template::new()
///     .name("player")
///     .with(Transform::from_xy(0.0, 0.0))
///     .with(Sprite::new().color(Color::GREEN).size(40.0, 40.0))
///     .child(Template::new()
///         .with(Transform::from_xy(0.0, 20.0))
///         .with(Sprite::new().color(Color::WHITE).size(10.0, 5.0))
///     );
///
/// player.spawn(&mut world);
/// ```
pub struct Template {
    name: Option<String>,
    tag: Option<String>,
    components: Vec<Box<dyn ComponentAdder>>,
    children: Vec<Template>,
}

impl Template {
    /// Create a new empty template.
    pub fn new() -> Self {
        Self {
            name: None,
            tag: None,
            components: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Set the entity's name (for lookup via `world.named()`).
    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    /// Add a component to this template.
    pub fn with<T: Clone + Send + Sync + 'static>(mut self, component: T) -> Self {
        self.components.push(Box::new(TypedAdder { value: component }));
        self
    }

    /// Tag the entity with a string label.
    pub fn tag(mut self, tag: &str) -> Self {
        self.tag = Some(tag.to_string());
        self
    }

    /// Add a child template that will be spawned as a child entity.
    pub fn child(mut self, child: Template) -> Self {
        self.children.push(child);
        self
    }

    /// Spawn this template into the world, returning the root entity.
    ///
    /// Children are spawned recursively with proper `Parent`/`Children`/
    /// `GlobalTransform` relationships.
    pub fn spawn(&self, world: &mut World) -> Entity {
        self.spawn_inner(world, None)
    }

    fn spawn_inner(&self, world: &mut World, parent: Option<Entity>) -> Entity {
        let entity = world.spawn_empty();

        // Set name if provided.
        if let Some(ref name) = self.name {
            world.name_entity(entity, name);
        }

        // Insert all components.
        for adder in &self.components {
            adder.insert_into(world, entity);
        }

        // Tag if provided.
        if let Some(ref tag) = self.tag {
            world.tag(entity, tag);
        }

        // Set up parent-child relationship.
        if let Some(parent) = parent {
            world.insert(entity, Parent(parent));
            world.insert(entity, GlobalTransform::default());
            if let Some(children) = world.get_mut::<Children>(parent) {
                children.0.push(entity);
            } else {
                world.insert(parent, Children(vec![entity]));
            }
        }

        // Spawn children recursively.
        for child_template in &self.children {
            child_template.spawn_inner(world, Some(entity));
        }

        entity
    }
}

impl Clone for Template {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            tag: self.tag.clone(),
            components: self.components.iter().map(|c| c.clone_box()).collect(),
            children: self.children.clone(),
        }
    }
}

impl Default for Template {
    fn default() -> Self {
        Self::new()
    }
}

// ── SceneBuilder ───────────────────────────────────────────────────────

/// Compose [`Template`]s into a named scene with optional lifecycle callbacks.
///
/// All entities spawned by a scene are tagged with [`SceneMarker`] for cleanup.
///
/// # Example
///
/// ```ignore
/// let menu = SceneBuilder::new("menu")
///     .add(title_template)
///     .add(play_button_template)
///     .on_enter(|ctx| { log::info!("Entered menu"); })
///     .on_exit(|ctx| { log::info!("Left menu"); });
/// ```
pub struct SceneBuilder {
    name: String,
    templates: Vec<Template>,
    on_enter: Option<SceneCallback>,
    on_exit: Option<SceneCallback>,
}

impl SceneBuilder {
    /// Create a new scene builder with the given name.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            templates: Vec::new(),
            on_enter: None,
            on_exit: None,
        }
    }

    /// Add a template to this scene.
    pub fn add(mut self, template: Template) -> Self {
        self.templates.push(template);
        self
    }

    /// Set a callback that runs when this scene is entered.
    pub fn on_enter(mut self, f: impl FnMut(&mut Context) + Send + 'static) -> Self {
        self.on_enter = Some(Mutex::new(Box::new(f)));
        self
    }

    /// Set a callback that runs when this scene is exited.
    pub fn on_exit(mut self, f: impl FnMut(&mut Context) + Send + 'static) -> Self {
        self.on_exit = Some(Mutex::new(Box::new(f)));
        self
    }
}

// ── SceneManager ───────────────────────────────────────────────────────

/// Scene lifecycle manager. Registered as a world resource by the [`Scenes`] plugin.
///
/// Queue a scene transition with [`goto`](SceneManager::goto). The transition
/// is processed at the start of the next frame.
pub struct SceneManager {
    scenes: Vec<SceneEntry>,
    active: Option<String>,
    pending: Option<String>,
}

struct SceneEntry {
    name: String,
    templates: Vec<Template>,
    on_enter: Option<SceneCallback>,
    on_exit: Option<SceneCallback>,
}

impl SceneManager {
    fn new() -> Self {
        Self {
            scenes: Vec::new(),
            active: None,
            pending: None,
        }
    }

    fn register(&mut self, builder: SceneBuilder) {
        self.scenes.push(SceneEntry {
            name: builder.name,
            templates: builder.templates,
            on_enter: builder.on_enter,
            on_exit: builder.on_exit,
        });
    }

    /// Queue a transition to the named scene. Processed at the start of the
    /// next frame.
    pub fn goto(&mut self, name: &str) {
        self.pending = Some(name.to_string());
    }

    /// The name of the currently active scene, or `None`.
    pub fn active(&self) -> Option<&str> {
        self.active.as_deref()
    }
}

impl std::fmt::Debug for SceneManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SceneManager")
            .field("active", &self.active)
            .field("pending", &self.pending)
            .field("scene_count", &self.scenes.len())
            .finish()
    }
}

// ── Transition System ──────────────────────────────────────────────────

/// Process pending scene transitions.
///
/// Uses the extract/reinsert pattern: removes `SceneManager` from the world,
/// operates on it, then re-inserts it.
fn scene_transition_system(ctx: &mut Context) {
    let Some(mut manager) = ctx.world.resource_remove::<SceneManager>() else {
        return;
    };

    let Some(target) = manager.pending.take() else {
        ctx.world.insert_resource(manager);
        return;
    };

    // Skip if already on the target scene.
    if manager.active.as_deref() == Some(&target) {
        ctx.world.insert_resource(manager);
        return;
    }

    // Run exit callback for current scene.
    if let Some(ref active_name) = manager.active {
        if let Some(entry) = manager.scenes.iter_mut().find(|s| s.name == *active_name) {
            if let Some(ref on_exit) = entry.on_exit {
                if let Ok(mut cb) = on_exit.lock() {
                    cb(ctx);
                }
            }
        }

        // Unload current scene entities.
        let mut to_despawn = Vec::new();
        ctx.world.query::<(&SceneMarker,)>(|entity, (marker,)| {
            if marker.0 == *active_name {
                to_despawn.push(entity);
            }
        });
        for entity in to_despawn {
            ctx.world.despawn_recursive(entity);
        }
    }

    // Find the target scene and spawn its templates.
    let scene_idx = manager.scenes.iter().position(|s| s.name == target);
    if let Some(idx) = scene_idx {
        // Spawn templates.
        let templates: Vec<Template> = manager.scenes[idx].templates.iter().map(|t| t.clone()).collect();
        for template in &templates {
            let entity = template.spawn(&mut ctx.world);
            ctx.world.insert(entity, SceneMarker(target.clone()));
        }

        // Run enter callback.
        if let Some(ref on_enter) = manager.scenes[idx].on_enter {
            if let Ok(mut cb) = on_enter.lock() {
                cb(ctx);
            }
        }
    } else {
        log::warn!("SceneManager: no scene named '{}'", target);
    }

    manager.active = Some(target);
    ctx.world.insert_resource(manager);
}

// ── Scenes Plugin ──────────────────────────────────────────────────────

/// Plugin that registers [`SceneManager`] as a resource and adds the transition
/// processing system.
///
/// # Example
///
/// ```ignore
/// Game::new("My Game")
///     .plugin(Scenes::new()
///         .add(menu_scene)
///         .add(game_scene)
///         .start("menu")
///     )
///     .update(update)
///     .run();
///
/// // In a system:
/// ctx.world.resource_mut::<SceneManager>().goto("game");
/// ```
pub struct Scenes {
    /// Interior mutability so `build(&self)` can take ownership of builders.
    builders: RefCell<Vec<SceneBuilder>>,
    start: Option<String>,
}

impl Scenes {
    /// Create a new Scenes plugin.
    pub fn new() -> Self {
        Self {
            builders: RefCell::new(Vec::new()),
            start: None,
        }
    }

    /// Add a scene.
    pub fn add(self, builder: SceneBuilder) -> Self {
        self.builders.borrow_mut().push(builder);
        self
    }

    /// Set the initial scene to load on startup.
    pub fn start(mut self, name: &str) -> Self {
        self.start = Some(name.to_string());
        self
    }
}

impl Default for Scenes {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::game::Plugin for Scenes {
    fn build(&self, game: &mut crate::game::Game) {
        let mut manager = SceneManager::new();

        // Take ownership of all builders via RefCell.
        let builders = self.builders.borrow_mut().drain(..).collect::<Vec<_>>();
        for builder in builders {
            manager.register(builder);
        }

        // Queue the initial scene if set.
        if let Some(ref start) = self.start {
            manager.pending = Some(start.clone());
        }

        game.insert_resource(manager);
        game.add_update_system(scene_transition_system);
    }
}
