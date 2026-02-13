//! Render pass orchestration.
//!
//! When both `render2d` and `render3d` features are enabled, runtime dispatch
//! picks the 3D path if a `Camera3d` component exists, otherwise the 2D path.
//! Falls back to a simple clear pass when neither feature is enabled.

use crate::ecs::World;

/// The clear color resource. Set this to change the background color.
#[derive(Debug, Clone, Copy)]
pub struct ClearColor(pub [f64; 4]);

impl Default for ClearColor {
    fn default() -> Self {
        // A pleasant dark blue, like a night sky.
        Self([0.1, 0.1, 0.15, 1.0])
    }
}

/// Render a single frame. Dispatches to 2D or 3D renderer based on the scene.
pub(crate) fn render_frame(world: &mut World) -> Result<(), wgpu::SurfaceError> {
    #[cfg(all(feature = "render2d", feature = "render3d"))]
    {
        if world.has_component_type::<crate::render3d::Camera3d>() {
            return crate::render3d::draw::render_meshes_3d(world);
        }
        return crate::render2d::draw::render_sprites_2d(world);
    }

    #[cfg(all(feature = "render2d", not(feature = "render3d")))]
    {
        return crate::render2d::draw::render_sprites_2d(world);
    }

    #[cfg(all(not(feature = "render2d"), feature = "render3d"))]
    {
        return crate::render3d::draw::render_meshes_3d(world);
    }

    #[cfg(all(not(feature = "render2d"), not(feature = "render3d")))]
    {
        let clear_color = world
            .get_resource::<ClearColor>()
            .copied()
            .unwrap_or_default();
        let gpu = world.resource::<super::gpu::GpuContext>();

        let output = gpu.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("kera frame encoder"),
            });

        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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

        gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
