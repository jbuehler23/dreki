//! Window management via winit.
//!
//! Implements [`winit::application::ApplicationHandler`] to drive the event
//! loop. This handles window creation, input forwarding, resize, and the
//! main game loop (systems + rendering each frame).

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

use crate::asset::process_asset_reloads;
use crate::context::Context;
use crate::ecs::hierarchy::propagate_transforms;
use crate::ecs::world::World;
use crate::render::gpu::GpuContext;
use crate::render::pass::{render_frame, FrameContext};

/// The application state that winit drives.
pub(crate) struct WinitApp {
    ctx: Context,
    startup_systems: Vec<Box<dyn FnMut(&mut Context)>>,
    systems: Vec<Box<dyn FnMut(&mut Context)>>,
    window: Option<Arc<Window>>,
    started: bool,
    title: String,
    #[cfg(feature = "editor")]
    editor: Option<crate::editor::EditorState>,
}

impl WinitApp {
    pub fn new(
        ctx: Context,
        startup_systems: Vec<Box<dyn FnMut(&mut Context)>>,
        systems: Vec<Box<dyn FnMut(&mut Context)>>,
        title: String,
    ) -> Self {
        Self {
            ctx,
            startup_systems,
            systems,
            window: None,
            started: false,
            title,
            #[cfg(feature = "editor")]
            editor: None,
        }
    }
}

impl ApplicationHandler for WinitApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = Window::default_attributes()
                .with_title(&self.title)
                .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));
            let window = Arc::new(
                event_loop
                    .create_window(attrs)
                    .expect("Failed to create window"),
            );

            // Initialize GPU.
            let gpu = GpuContext::new(window.clone());
            self.ctx.world.insert_resource(gpu);

            // Initialize editor if the feature is enabled.
            #[cfg(feature = "editor")]
            {
                let gpu = self.ctx.world.resource::<GpuContext>();
                self.editor = Some(crate::editor::EditorState::new(gpu, &window));
            }

            self.window = Some(window);
        }

        // Run startup systems once.
        if !self.started {
            self.started = true;
            for system in self.startup_systems.iter_mut() {
                system(&mut self.ctx);
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Forward events to egui editor.
        #[cfg(feature = "editor")]
        {
            if let Some(window) = &self.window {
                if let Some(editor) = &mut self.editor {
                    if editor.on_window_event(window, &event) {
                        return;
                    }
                }
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                log::info!("Window close requested, exiting.");
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.ctx.world.get_resource_mut::<GpuContext>() {
                    gpu.resize(size.width, size.height);
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                // Toggle editor with F12.
                #[cfg(feature = "editor")]
                {
                    if let PhysicalKey::Code(winit::keyboard::KeyCode::F12) = event.physical_key {
                        if event.state == ElementState::Pressed && !event.repeat {
                            if let Some(editor) = &mut self.editor {
                                editor.visible = !editor.visible;
                                log::info!("Editor {}", if editor.visible { "opened" } else { "closed" });
                            }
                        }
                    }
                }

                if let PhysicalKey::Code(key_code) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => self.ctx.input.keys.press(key_code),
                        ElementState::Released => self.ctx.input.keys.release(key_code),
                    }
                }
            }

            WindowEvent::MouseInput { button, state, .. } => match state {
                ElementState::Pressed => self.ctx.input.mouse.press(button),
                ElementState::Released => self.ctx.input.mouse.release(button),
            },

            WindowEvent::CursorMoved { position, .. } => {
                self.ctx.cursor.x = position.x as f32;
                self.ctx.cursor.y = position.y as f32;
            }

            WindowEvent::RedrawRequested => {
                // Update timing.
                self.ctx.time.update();
                // Sync Time to world resource (physics systems read it from here).
                self.ctx.world.insert_resource(self.ctx.time);

                // Process any pending asset hot-reloads.
                process_asset_reloads(&mut self.ctx.world);

                // Run game systems.
                #[cfg(feature = "diagnostics")]
                let _systems_start = std::time::Instant::now();
                for system in self.systems.iter_mut() {
                    system(&mut self.ctx);
                }

                // Clear per-frame input state.
                self.ctx.input.keys.clear_just();
                self.ctx.input.mouse.clear_just();

                // Propagate parent→child transforms so GlobalTransform is up to date.
                propagate_transforms(&mut self.ctx.world);

                // Build editor UI (must happen before render so paint jobs are ready).
                #[cfg(feature = "editor")]
                {
                    if let Some(window) = &self.window {
                        if let Some(editor) = &mut self.editor {
                            editor.build_ui(&mut self.ctx.world, window);
                        }
                    }
                }

                // Render (with editor overlay when enabled).
                #[cfg(feature = "editor")]
                {
                    let editor = &mut self.editor;
                    render_world(event_loop, &mut self.ctx.world, |frame| {
                        if let Some(ed) = editor.as_mut() {
                            ed.render_overlay(frame);
                        }
                    });
                }
                #[cfg(not(feature = "editor"))]
                {
                    render_world(event_loop, &mut self.ctx.world, |_| {});
                }

                // Request next frame.
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        #[cfg(feature = "diagnostics")]
        crate::diag::send_diagnostics(&mut self.ctx.world, &self.ctx.time);
    }
}

/// Render the world and handle surface errors.
fn render_world(
    event_loop: &ActiveEventLoop,
    world: &mut World,
    overlay: impl FnOnce(&mut FrameContext<'_>),
) {
    if world.has_resource::<GpuContext>() {
        match render_frame(world, overlay) {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                if let Some(gpu) = world.get_resource_mut::<GpuContext>() {
                    let (w, h) = gpu.surface_size();
                    gpu.resize(w, h);
                }
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                log::error!("Out of GPU memory!");
                event_loop.exit();
            }
            Err(e) => {
                log::warn!("Surface error: {:?}", e);
            }
        }
    }
}
