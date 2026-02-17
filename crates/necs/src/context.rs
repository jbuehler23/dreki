//! Context — the main game context passed to all systems.
//!
//! [`Context`] bundles the ECS [`World`], input state, cursor position, and
//! frame timing into a single struct. Startup and update systems receive
//! `&mut Context`, giving them access to everything they need.

use crate::ecs::world::World;
use crate::ecs::Entity;
use crate::input::{CursorPosition, Input, KeyCode, MouseButton};
use crate::time::Time;

// ── InputState ──────────────────────────────────────────────────────────

/// Wraps keyboard and mouse input with convenience methods.
///
/// Access via [`Context::input`].
pub struct InputState {
    pub(crate) keys: Input<KeyCode>,
    pub(crate) mouse: Input<MouseButton>,
}

impl InputState {
    pub(crate) fn new() -> Self {
        Self {
            keys: Input::new(),
            mouse: Input::new(),
        }
    }

    /// Returns `true` if the key is currently held down.
    pub fn pressed(&self, key: KeyCode) -> bool {
        self.keys.pressed(key)
    }

    /// Returns `true` if the key was pressed this frame.
    pub fn just_pressed(&self, key: KeyCode) -> bool {
        self.keys.just_pressed(key)
    }

    /// Returns `true` if the key was released this frame.
    pub fn just_released(&self, key: KeyCode) -> bool {
        self.keys.just_released(key)
    }

    /// Returns `true` if the mouse button is currently held down.
    pub fn mouse_pressed(&self, button: MouseButton) -> bool {
        self.mouse.pressed(button)
    }

    /// Returns `true` if the mouse button was pressed this frame.
    pub fn mouse_just_pressed(&self, button: MouseButton) -> bool {
        self.mouse.just_pressed(button)
    }

    /// Returns `true` if the mouse button was released this frame.
    pub fn mouse_just_released(&self, button: MouseButton) -> bool {
        self.mouse.just_released(button)
    }
}

// ── Context ──────────────────────────────────────────────────────────────

/// The main game context, passed to all startup and update systems.
///
/// Provides access to the ECS world, input state, cursor position, and timing.
///
/// # Example
///
/// ```ignore
/// fn setup(ctx: &mut Context) {
///     ctx.spawn("camera")
///         .insert(Transform::default())
///         .insert(Camera2d);
/// }
///
/// fn update(ctx: &mut Context) {
///     let dt = ctx.time.delta_secs();
///     if ctx.input.pressed(KeyCode::KeyW) {
///         // move something
///     }
/// }
/// ```
pub struct Context {
    /// The ECS world containing all entities, components, and resources.
    pub world: World,
    /// Keyboard and mouse input state.
    pub input: InputState,
    /// Mouse cursor position in window coordinates.
    pub cursor: CursorPosition,
    /// Frame timing (delta time, elapsed time, FPS).
    pub time: Time,
}

impl Context {
    pub(crate) fn new() -> Self {
        let mut world = World::new();
        let time = Time::new();
        world.insert_resource(time);
        world.insert_resource(crate::asset::AssetServer::new());

        Self {
            world,
            input: InputState::new(),
            cursor: CursorPosition::default(),
            time,
        }
    }

    /// Spawn a named entity. Returns an [`EntityBuilder`] for adding components.
    ///
    /// The name can later be used to look up the entity with
    /// [`world.named()`](World::named).
    pub fn spawn(&mut self, name: &str) -> EntityBuilder<'_> {
        let entity = self.world.spawn_empty();
        self.world.name_entity(entity, name);
        EntityBuilder {
            world: &mut self.world,
            entity,
        }
    }

    /// Create an unnamed entity. Returns an [`EntityBuilder`] for adding components.
    pub fn create(&mut self) -> EntityBuilder<'_> {
        let entity = self.world.spawn_empty();
        EntityBuilder {
            world: &mut self.world,
            entity,
        }
    }

    /// Load a 2D texture from disk and return a handle.
    #[cfg(feature = "render2d")]
    pub fn load_texture(&mut self, path: &str) -> crate::render2d::TextureHandle {
        crate::render2d::texture::load_texture(&mut self.world, path)
    }

    /// Load a font from disk at the given pixel size and return a handle.
    #[cfg(feature = "render2d")]
    pub fn load_font(&mut self, path: &str, size: f32) -> crate::render2d::FontHandle {
        crate::render2d::font::load_font(&mut self.world, path, size)
    }

    /// Create a texture from raw RGBA8 pixel data and return a handle.
    #[cfg(feature = "render2d")]
    pub fn create_texture(
        &mut self,
        label: &str,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> crate::render2d::TextureHandle {
        crate::render2d::texture::create_texture_from_rgba(&mut self.world, label, width, height, data)
    }

    /// Load a 3D texture from disk and return a handle.
    #[cfg(feature = "render3d")]
    pub fn load_texture_3d(&mut self, path: &str) -> crate::render3d::TextureHandle3d {
        crate::render3d::texture::load_texture_3d(&mut self.world, path)
    }
}

// ── EntityBuilder ────────────────────────────────────────────────────────

/// Builder for adding components to a freshly spawned entity.
///
/// Returned by [`Context::spawn`] and [`Context::create`]. Chain `.insert()`
/// calls to add components, and optionally `.tag()` to tag the entity.
///
/// # Example
///
/// ```ignore
/// ctx.spawn("player")
///     .insert(Transform::from_xy(0.0, 0.0))
///     .insert(Sprite::new().color(Color::GREEN).size(40.0, 40.0))
///     .tag("player");
/// ```
pub struct EntityBuilder<'w> {
    world: &'w mut World,
    entity: Entity,
}

impl<'w> EntityBuilder<'w> {
    /// Add a component to this entity.
    pub fn insert<T: 'static + Send + Sync>(self, component: T) -> Self {
        self.world.insert(self.entity, component);
        self
    }

    /// Tag this entity with a string label.
    pub fn tag(self, tag: &str) -> Self {
        self.world.tag(self.entity, tag);
        self
    }

    /// Get the entity ID.
    pub fn id(&self) -> Entity {
        self.entity
    }
}
