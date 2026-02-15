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
use crate::ecs::system::Schedule;
use crate::ecs::world::World;
use crate::input::{CursorPosition, Input, KeyCode, MouseButton};
use crate::render::gpu::GpuContext;
use crate::render::pass::render_frame;
use crate::time::Time;

/// The application state that winit drives.
pub(crate) struct WinitApp {
    pub world: World,
    pub startup_systems: Schedule,
    pub systems: Schedule,
    pub window: Option<Arc<Window>>,
    pub started: bool,
    pub title: String,
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
            self.world.insert_resource(gpu);

            self.window = Some(window);
        }

        // Run startup systems once.
        if !self.started {
            self.started = true;
            self.startup_systems.run(&mut self.world);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                log::info!("Window close requested, exiting.");
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.world.get_resource_mut::<GpuContext>() {
                    gpu.resize(size.width, size.height);
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key_code) = event.physical_key {
                    if let Some(input) = self.world.get_resource_mut::<Input<KeyCode>>() {
                        match event.state {
                            ElementState::Pressed => input.press(key_code),
                            ElementState::Released => input.release(key_code),
                        }
                    }
                }
            }

            WindowEvent::MouseInput { button, state, .. } => {
                if let Some(input) = self.world.get_resource_mut::<Input<MouseButton>>() {
                    match state {
                        ElementState::Pressed => input.press(button),
                        ElementState::Released => input.release(button),
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                if let Some(cursor) = self.world.get_resource_mut::<CursorPosition>() {
                    cursor.x = position.x as f32;
                    cursor.y = position.y as f32;
                }
            }

            WindowEvent::RedrawRequested => {
                // Update timing.
                if let Some(time) = self.world.get_resource_mut::<Time>() {
                    time.update();
                }

                // Process any pending asset hot-reloads.
                process_asset_reloads(&mut self.world);

                // Run game systems.
                #[cfg(feature = "diagnostics")]
                let systems_start = std::time::Instant::now();
                self.systems.run(&mut self.world);
                #[cfg(feature = "diagnostics")]
                let systems_elapsed = systems_start.elapsed();

                // Copy per-system timings into a resource.
                #[cfg(feature = "diagnostics")]
                {
                    let timings: Vec<crate::ecs::system::SystemTiming> = self
                        .systems
                        .timings
                        .drain(..)
                        .collect();
                    self.world
                        .insert_resource(crate::diag::SystemTimings(timings));
                }

                // Clear per-frame input state.
                if let Some(input) = self.world.get_resource_mut::<Input<KeyCode>>() {
                    input.clear_just();
                }
                if let Some(input) = self.world.get_resource_mut::<Input<MouseButton>>() {
                    input.clear_just();
                }

                // Render.
                #[cfg(feature = "diagnostics")]
                let render_start = std::time::Instant::now();
                if self.world.has_resource::<GpuContext>() {
                    match render_frame(&mut self.world) {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            // Reconfigure surface on lost/outdated.
                            if let Some(gpu) = self.world.get_resource_mut::<GpuContext>() {
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
                #[cfg(feature = "diagnostics")]
                {
                    let render_elapsed = render_start.elapsed();
                    self.world.insert_resource(crate::diag::FrameBudget {
                        systems_us: systems_elapsed.as_secs_f64() * 1_000_000.0,
                        render_us: render_elapsed.as_secs_f64() * 1_000_000.0,
                    });
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
        crate::diag::send_diagnostics(&mut self.world);
    }
}
