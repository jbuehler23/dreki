//! In-engine editor overlay, toggled with F12.
//!
//! Feature-gated behind `#[cfg(feature = "editor")]`. Provides an entity
//! hierarchy, component inspector, and toolbar using egui.
//!
//! The [`EditorState`] is stored directly in `WinitApp` rather than as a World
//! resource because `egui_winit::State` is not `Sync`.

mod hierarchy;
mod inspector;
mod toolbar;

use std::sync::Arc;

use crate::ecs::Entity;
use crate::ecs::world::World;
use crate::render::gpu::GpuContext;
use crate::render::pass::FrameContext;

/// Editor state. Stored in WinitApp, not in World (because egui_winit is !Sync).
pub struct EditorState {
    pub egui_ctx: egui::Context,
    pub egui_winit: egui_winit::State,
    pub egui_renderer: egui_wgpu::Renderer,
    /// Whether the editor overlay is visible.
    pub visible: bool,
    /// The currently selected entity in the hierarchy panel.
    pub selected: Option<Entity>,
    /// Prepared paint jobs for the current frame.
    paint_jobs: Vec<egui::ClippedPrimitive>,
    /// Textures delta for the current frame.
    textures_delta: egui::TexturesDelta,
    /// Whether paint jobs are ready for rendering.
    frame_ready: bool,
}

impl EditorState {
    /// Create a new editor state.
    pub fn new(gpu: &GpuContext, window: &Arc<winit::window::Window>) -> Self {
        let egui_ctx = egui::Context::default();

        let egui_winit = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,
            Some(gpu.device.limits().max_texture_dimension_2d as usize),
        );

        let egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.surface_format(),
            egui_wgpu::RendererOptions::default(),
        );

        Self {
            egui_ctx,
            egui_winit,
            egui_renderer,
            visible: false,
            selected: None,
            paint_jobs: Vec::new(),
            textures_delta: egui::TexturesDelta::default(),
            frame_ready: false,
        }
    }

    /// Forward a winit event to egui. Returns true if egui consumed the event.
    pub fn on_window_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) -> bool {
        if !self.visible {
            return false;
        }
        let response = self.egui_winit.on_window_event(window, event);
        response.consumed
    }

    /// Build the editor UI for this frame.
    pub fn build_ui(
        &mut self,
        world: &mut World,
        window: &winit::window::Window,
    ) {
        if !self.visible {
            self.frame_ready = false;
            return;
        }

        let raw_input = self.egui_winit.take_egui_input(window);
        let selected = self.selected;
        let mut new_selected = selected;

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            toolbar::toolbar_panel(ctx);
            new_selected = hierarchy::hierarchy_panel(ctx, world, selected);
            inspector::inspector_panel(ctx, world, new_selected);
        });

        self.selected = new_selected;

        self.egui_winit
            .handle_platform_output(window, full_output.platform_output);

        self.paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        self.textures_delta = full_output.textures_delta;
        self.frame_ready = true;
    }

    /// Render the editor overlay into the current frame.
    pub fn render_overlay(&mut self, frame: &mut FrameContext<'_>) {
        if !self.frame_ready {
            return;
        }
        self.frame_ready = false;

        let gpu = frame.gpu;
        let (sw, sh) = gpu.surface_size();

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [sw, sh],
            pixels_per_point: self.egui_ctx.pixels_per_point(),
        };

        // Update textures.
        for (id, delta) in &self.textures_delta.set {
            self.egui_renderer
                .update_texture(&gpu.device, &gpu.queue, *id, delta);
        }

        // Update buffers.
        let cmd_buffers = self.egui_renderer.update_buffers(
            &gpu.device,
            &gpu.queue,
            &mut frame.encoder,
            &self.paint_jobs,
            &screen_descriptor,
        );

        // Submit extra command buffers from buffer updates.
        if !cmd_buffers.is_empty() {
            gpu.queue.submit(cmd_buffers);
        }

        // Render egui overlay.
        {
            let render_pass = frame.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.egui_renderer.render(
                &mut render_pass.forget_lifetime(),
                &self.paint_jobs,
                &screen_descriptor,
            );
        }

        // Free textures.
        for id in &self.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
    }
}
