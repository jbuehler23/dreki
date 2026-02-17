//! Render pass orchestration.
//!
//! When both `render2d` and `render3d` features are enabled, runtime dispatch
//! picks the 3D path if a `Camera3d` component exists, otherwise the 2D path.
//! Falls back to a simple clear pass when neither feature is enabled.

use crate::ecs::World;
use crate::render::gpu::GpuContext;

/// The clear color resource. Set this to change the background color.
#[derive(Debug, Clone, Copy)]
pub struct ClearColor(pub [f64; 4]);

impl Default for ClearColor {
    fn default() -> Self {
        // A pleasant dark blue, like a night sky.
        Self([0.1, 0.1, 0.15, 1.0])
    }
}

/// Per-frame render context passed to 2D/3D renderers.
///
/// Created by [`render_frame`], which acquires the surface texture and encoder.
/// The renderers add their passes to the encoder; submit/present happens after
/// all passes (including optional editor overlay) are recorded.
pub(crate) struct FrameContext<'a> {
    pub encoder: wgpu::CommandEncoder,
    pub view: wgpu::TextureView,
    pub gpu: &'a GpuContext,
}

/// Render a single frame. Dispatches to 2D or 3D renderer based on the scene.
///
/// The `overlay` callback is invoked after the main scene render pass but before
/// submit/present — use it to add the editor overlay or other post-scene passes.
pub(crate) fn render_frame(
    world: &mut World,
    overlay: impl FnOnce(&mut FrameContext<'_>),
) -> Result<(), wgpu::SurfaceError> {
    let gpu = world
        .resource_remove::<GpuContext>()
        .expect("GpuContext missing");

    let output = gpu.surface.get_current_texture()?;
    let view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("necs frame encoder"),
        });

    let mut frame = FrameContext {
        encoder,
        view,
        gpu: &gpu,
    };

    // Dispatch to the appropriate renderer.
    #[cfg(all(feature = "render2d", feature = "render3d"))]
    {
        if world.has_component_type::<crate::render3d::Camera3d>() {
            crate::render3d::draw::render_meshes_3d(world, &mut frame);
        } else {
            crate::render2d::draw::render_sprites_2d(world, &mut frame);
        }
    }

    #[cfg(all(feature = "render2d", not(feature = "render3d")))]
    {
        crate::render2d::draw::render_sprites_2d(world, &mut frame);
    }

    #[cfg(all(not(feature = "render2d"), feature = "render3d"))]
    {
        crate::render3d::draw::render_meshes_3d(world, &mut frame);
    }

    #[cfg(all(not(feature = "render2d"), not(feature = "render3d")))]
    {
        let clear_color = world
            .get_resource::<ClearColor>()
            .copied()
            .unwrap_or_default();

        {
            let _render_pass = frame.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color.0[0],
                            g: clear_color.0[1],
                            b: clear_color.0[2],
                            a: clear_color.0[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
    }

    // Apply overlay (editor, debug visualizations, etc.)
    overlay(&mut frame);

    // Submit all recorded passes and present.
    gpu.queue.submit(std::iter::once(frame.encoder.finish()));
    output.present();

    world.insert_resource(gpu);

    Ok(())
}
