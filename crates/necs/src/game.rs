//! Game builder and plugin system.
//!
//! [`Game`] is the main entry point for a necs application. Configure
//! resources, systems, and plugins, then call [`run`](Game::run) to start
//! the event loop.
//!
//! # Example
//!
//! ```ignore
//! use necs::prelude::*;
//!
//! fn main() {
//!     Game::new("My Game")
//!         .resource(ClearColor([0.1, 0.1, 0.15, 1.0]))
//!         .setup(setup)
//!         .update(update)
//!         .run();
//! }
//!
//! fn setup(ctx: &mut Context) {
//!     ctx.spawn("camera").insert(Transform::default()).insert(Camera2d);
//! }
//!
//! fn update(ctx: &mut Context) {
//!     let dt = ctx.time.delta_secs();
//!     // game logic here
//! }
//! ```

use crate::context::Context;

/// A plugin that can extend a [`Game`] with additional systems and resources.
///
/// Implement this trait to bundle related resources and systems together.
///
/// # Example
///
/// ```ignore
/// pub struct MyPlugin;
///
/// impl Plugin for MyPlugin {
///     fn build(&self, game: &mut Game) {
///         game.insert_resource(MyResource::new());
///         game.add_update_system(|ctx| {
///             // system logic
///         });
///     }
/// }
/// ```
pub trait Plugin {
    fn build(&self, game: &mut Game);
}

/// The main game builder. Configure resources, systems, and plugins, then
/// call [`run`](Game::run) to start the event loop.
pub struct Game {
    title: String,
    ctx: Context,
    startup_systems: Vec<Box<dyn FnMut(&mut Context)>>,
    update_systems: Vec<Box<dyn FnMut(&mut Context)>>,
}

impl Game {
    /// Create a new game with the given window title.
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            ctx: Context::new(),
            startup_systems: Vec::new(),
            update_systems: Vec::new(),
        }
    }

    /// Insert a resource into the world (builder pattern).
    pub fn resource<T: 'static + Send + Sync>(mut self, value: T) -> Self {
        self.ctx.world.insert_resource(value);
        self
    }

    /// Register a startup system that runs once after window creation.
    pub fn setup(mut self, system: fn(&mut Context)) -> Self {
        self.startup_systems.push(Box::new(system));
        self
    }

    /// Register an update system that runs every frame.
    pub fn update(mut self, system: fn(&mut Context)) -> Self {
        self.update_systems.push(Box::new(system));
        self
    }

    /// Register a world system (takes `&mut World` instead of `&mut Context`).
    ///
    /// This wraps the system to work with the Context-based API. Prefer using
    /// plugins and Context-based systems for new code.
    pub fn world_system(mut self, system: fn(&mut crate::ecs::World)) -> Self {
        self.update_systems.push(Box::new(move |ctx: &mut Context| {
            system(&mut ctx.world);
        }));
        self
    }

    /// Apply a plugin, which can register resources and systems.
    pub fn plugin(mut self, plugin: impl Plugin) -> Self {
        plugin.build(&mut self);
        self
    }

    /// Insert a resource (non-consuming, for use by plugins).
    pub fn insert_resource<T: 'static + Send + Sync>(&mut self, value: T) {
        self.ctx.world.insert_resource(value);
    }

    /// Register a startup system (non-consuming, for use by plugins).
    pub fn add_startup_system(&mut self, system: impl FnMut(&mut Context) + 'static) {
        self.startup_systems.push(Box::new(system));
    }

    /// Register an update system (non-consuming, for use by plugins).
    pub fn add_update_system(&mut self, system: impl FnMut(&mut Context) + 'static) {
        self.update_systems.push(Box::new(system));
    }

    /// Start the event loop. This function does not return.
    pub fn run(self) {
        let event_loop = winit::event_loop::EventLoop::new()
            .expect("Failed to create event loop");

        let mut app = crate::window::WinitApp::new(
            self.ctx,
            self.startup_systems,
            self.update_systems,
            self.title,
        );

        event_loop.run_app(&mut app).expect("Event loop error");
    }
}
