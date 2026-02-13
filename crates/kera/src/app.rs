//! App builder and plugin system.
//!
//! The [`App`] is the entry point for a kera game. It provides a builder
//! pattern for registering plugins, resources, and systems, then runs the
//! event loop.
//!
//! ## Example
//!
//! ```ignore
//! use kera::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .insert_resource(ClearColor([0.2, 0.3, 0.8, 1.0]))
//!         .add_startup_system(setup)
//!         .add_system(move_player)
//!         .run();
//! }
//! ```

use winit::event_loop::EventLoop;

use crate::asset::AssetServer;
use crate::ecs::system::Schedule;
use crate::ecs::world::World;
use crate::input::{CursorPosition, Input, KeyCode, MouseButton};
use crate::render::pass::ClearColor;
use crate::time::Time;
use crate::window::WinitApp;

/// A plugin can add resources, systems, and other configuration to the app.
pub trait Plugin: Send + Sync {
    fn build(&self, app: &mut App);
}

/// The app builder. Configure your game, then call [`run()`](App::run).
pub struct App {
    pub world: World,
    pub startup_systems: Schedule,
    pub systems: Schedule,
    title: String,
}

impl App {
    /// Create a new app with an empty world and no systems.
    pub fn new() -> Self {
        Self {
            world: World::new(),
            startup_systems: Schedule::new(),
            systems: Schedule::new(),
            title: String::from("kera"),
        }
    }

    /// Set the window title.
    pub fn set_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Insert a resource into the world.
    pub fn insert_resource<T: 'static + Send + Sync>(mut self, value: T) -> Self {
        self.world.insert_resource(value);
        self
    }

    /// Add a system that runs once at startup (after window creation).
    pub fn add_startup_system<S: crate::ecs::system::System + 'static>(
        mut self,
        system: S,
    ) -> Self {
        self.startup_systems.add_system(system);
        self
    }

    /// Add a system that runs every frame.
    pub fn add_system<S: crate::ecs::system::System + 'static>(mut self, system: S) -> Self {
        self.systems.add_system(system);
        self
    }

    /// Apply a plugin.
    pub fn add_plugins<P: Plugin>(mut self, plugin: P) -> Self {
        plugin.build(&mut self);
        self
    }

    /// Register a component type for debug formatting in diagnostics.
    ///
    /// Only effective when the `diagnostics` feature is enabled.
    #[cfg(feature = "diagnostics")]
    pub fn register_component<T: std::fmt::Debug + 'static + Send + Sync>(mut self) -> Self {
        self.world
            .resource_mut::<crate::diag::ComponentRegistry>()
            .register::<T>();
        self
    }

    /// Start the event loop. This function does not return.
    pub fn run(self) -> ! {
        let event_loop = EventLoop::new().expect("Failed to create event loop");
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

        let mut app = WinitApp {
            world: self.world,
            startup_systems: self.startup_systems,
            systems: self.systems,
            window: None,
            started: false,
            title: self.title,
        };

        event_loop.run_app(&mut app).expect("Event loop error");

        // winit's run_app returns when the event loop exits.
        std::process::exit(0);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

// ── Default Plugins ──────────────────────────────────────────────────────

/// The default plugin bundle. Registers core resources: Time, Input, ClearColor, etc.
pub struct DefaultPlugins;

impl Plugin for DefaultPlugins {
    fn build(&self, app: &mut App) {
        // Core resources.
        app.world.insert_resource(Time::new());
        app.world.insert_resource(Input::<KeyCode>::new());
        app.world.insert_resource(Input::<MouseButton>::new());
        app.world.insert_resource(CursorPosition::default());
        app.world.insert_resource(ClearColor::default());
        app.world.insert_resource(AssetServer::new());

        // Diagnostics (opt-in).
        #[cfg(feature = "diagnostics")]
        {
            crate::diag::init_logger();
            use crate::diag::{ComponentRegistry, DiagSender, RenderStats};

            if let Some(sender) = DiagSender::new() {
                app.world.insert_resource(sender);
            }
            let mut registry = ComponentRegistry::new();
            // Auto-register built-in types.
            registry.register::<crate::math::Transform>();
            #[cfg(feature = "render2d")]
            {
                registry.register::<crate::render2d::Camera2d>();
                registry.register::<crate::render2d::Sprite>();
                registry.register::<crate::render2d::Color>();
            }
            #[cfg(feature = "render3d")]
            {
                registry.register::<crate::render3d::Camera3d>();
                registry.register::<crate::render3d::Mesh3d>();
                registry.register::<crate::render3d::Material>();
                registry.register::<crate::render3d::DirectionalLight>();
                registry.register::<crate::render3d::PointLight>();
                registry.register::<crate::render3d::AmbientLight>();
            }
            #[cfg(feature = "physics2d")]
            {
                registry.register::<crate::physics2d::RigidBody2d>();
                registry.register::<crate::physics2d::Collider2d>();
            }
            #[cfg(feature = "physics3d")]
            {
                registry.register::<crate::physics3d::RigidBody3d>();
                registry.register::<crate::physics3d::Collider3d>();
            }
            app.world.insert_resource(registry);
            app.world.insert_resource(RenderStats::new());
        }
    }
}
