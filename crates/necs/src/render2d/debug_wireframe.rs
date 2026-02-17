//! Debug wireframe rendering for 2D physics colliders.
//!
//! Draws collider shapes as green line segments on top of the 2D scene.
//! Uses a separate LineList pipeline with no depth buffer (consistent with
//! 2D's painter's algorithm).

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::math::Vec2;
use crate::physics2d::ColliderShape2d;
use crate::render::gpu::GpuContext;

use super::pipeline::SpriteRenderer;

// ── Public resource ─────────────────────────────────────────────────────

/// Insert this resource to enable debug collider wireframes in 2D.
/// Toggle `enabled` at runtime (e.g. with F1).
#[derive(Debug)]
pub struct DebugColliders2d {
    pub enabled: bool,
    pub color: [f32; 4],
}

impl Default for DebugColliders2d {
    fn default() -> Self {
        Self {
            enabled: true,
            color: [0.0, 1.0, 0.0, 1.0], // green
        }
    }
}

// ── Vertex ──────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DebugVertex2d {
    position: [f32; 2],
}

impl DebugVertex2d {
    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<DebugVertex2d>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x2,
        }],
    };
}

// ── Renderer ────────────────────────────────────────────────────────────

pub(crate) struct DebugWireframeRenderer2d {
    pipeline: wgpu::RenderPipeline,
    color_buffer: wgpu::Buffer,
    color_bind_group: wgpu::BindGroup,
}

impl DebugWireframeRenderer2d {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("2d debug wireframe shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("debug_wireframe.wgsl").into()),
        });

        // Color uniform bind group layout (group 1)
        let color_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("2d debug color layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("2d debug wireframe pipeline layout"),
            bind_group_layouts: &[camera_bind_group_layout, &color_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("2d debug wireframe pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[DebugVertex2d::LAYOUT],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None, // 2D has no depth buffer
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let color_data: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
        let color_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("2d debug color buffer"),
            contents: bytemuck::cast_slice(&color_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let color_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("2d debug color bind group"),
            layout: &color_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: color_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            color_buffer,
            color_bind_group,
        }
    }
}

// ── Wireframe generators ────────────────────────────────────────────────

fn cuboid_wireframe_2d(hx: f32, hy: f32) -> Vec<DebugVertex2d> {
    let corners = [
        [-hx, -hy],
        [ hx, -hy],
        [ hx,  hy],
        [-hx,  hy],
    ];
    let edges: [(usize, usize); 4] = [(0, 1), (1, 2), (2, 3), (3, 0)];
    let mut verts = Vec::with_capacity(8);
    for (a, b) in edges {
        verts.push(DebugVertex2d { position: corners[a] });
        verts.push(DebugVertex2d { position: corners[b] });
    }
    verts
}

fn ball_wireframe_2d(radius: f32, segments: u32) -> Vec<DebugVertex2d> {
    let mut verts = Vec::with_capacity(segments as usize * 2);
    let step = std::f32::consts::TAU / segments as f32;
    for i in 0..segments {
        let a = i as f32 * step;
        let b = (i + 1) as f32 * step;
        verts.push(DebugVertex2d { position: [radius * a.cos(), radius * a.sin()] });
        verts.push(DebugVertex2d { position: [radius * b.cos(), radius * b.sin()] });
    }
    verts
}

fn capsule_wireframe_2d(half_height: f32, radius: f32, segments: u32, horizontal: bool) -> Vec<DebugVertex2d> {
    let mut verts = Vec::new();
    let half_seg = segments / 2;
    let half_step = std::f32::consts::PI / half_seg as f32;

    if horizontal {
        // Capsule along X axis
        // Top line
        verts.push(DebugVertex2d { position: [-half_height, radius] });
        verts.push(DebugVertex2d { position: [half_height, radius] });
        // Bottom line
        verts.push(DebugVertex2d { position: [-half_height, -radius] });
        verts.push(DebugVertex2d { position: [half_height, -radius] });
        // Right semicircle
        for i in 0..half_seg {
            let a = -(std::f32::consts::PI / 2.0) + i as f32 * half_step;
            let b = -(std::f32::consts::PI / 2.0) + (i + 1) as f32 * half_step;
            verts.push(DebugVertex2d { position: [half_height + radius * a.cos(), radius * a.sin()] });
            verts.push(DebugVertex2d { position: [half_height + radius * b.cos(), radius * b.sin()] });
        }
        // Left semicircle
        for i in 0..half_seg {
            let a = std::f32::consts::PI / 2.0 + i as f32 * half_step;
            let b = std::f32::consts::PI / 2.0 + (i + 1) as f32 * half_step;
            verts.push(DebugVertex2d { position: [-half_height + radius * a.cos(), radius * a.sin()] });
            verts.push(DebugVertex2d { position: [-half_height + radius * b.cos(), radius * b.sin()] });
        }
    } else {
        // Capsule along Y axis
        // Left line
        verts.push(DebugVertex2d { position: [-radius, -half_height] });
        verts.push(DebugVertex2d { position: [-radius, half_height] });
        // Right line
        verts.push(DebugVertex2d { position: [radius, -half_height] });
        verts.push(DebugVertex2d { position: [radius, half_height] });
        // Top semicircle
        for i in 0..half_seg {
            let a = i as f32 * half_step;
            let b = (i + 1) as f32 * half_step;
            verts.push(DebugVertex2d { position: [radius * a.cos(), half_height + radius * a.sin()] });
            verts.push(DebugVertex2d { position: [radius * b.cos(), half_height + radius * b.sin()] });
        }
        // Bottom semicircle
        for i in 0..half_seg {
            let a = std::f32::consts::PI + i as f32 * half_step;
            let b = std::f32::consts::PI + (i + 1) as f32 * half_step;
            verts.push(DebugVertex2d { position: [radius * a.cos(), -half_height + radius * a.sin()] });
            verts.push(DebugVertex2d { position: [radius * b.cos(), -half_height + radius * b.sin()] });
        }
    }
    verts
}

fn transform_vertices_2d(verts: &mut [DebugVertex2d], translation: Vec2, angle: f32) {
    let cos = angle.cos();
    let sin = angle.sin();
    for v in verts.iter_mut() {
        let x = v.position[0];
        let y = v.position[1];
        v.position[0] = cos * x - sin * y + translation.x;
        v.position[1] = sin * x + cos * y + translation.y;
    }
}

// ── Render entry point ──────────────────────────────────────────────────

const CIRCLE_SEGMENTS: u32 = 32;

pub(crate) fn render_debug_wireframes_2d(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    gpu: &GpuContext,
    renderer: &SpriteRenderer,
    debug_renderer: &mut DebugWireframeRenderer2d,
    debug_config: &DebugColliders2d,
    poses: &[(Vec2, f32, ColliderShape2d)],
) {
    if !debug_config.enabled {
        return;
    }

    // Update color uniform
    gpu.queue.write_buffer(
        &debug_renderer.color_buffer,
        0,
        bytemuck::cast_slice(&debug_config.color),
    );

    // Collect all wireframe vertices
    let mut all_verts: Vec<DebugVertex2d> = Vec::new();
    for &(translation, angle, shape) in poses {
        let mut shape_verts = match shape {
            ColliderShape2d::Cuboid { hx, hy } => cuboid_wireframe_2d(hx, hy),
            ColliderShape2d::Ball { radius } => ball_wireframe_2d(radius, CIRCLE_SEGMENTS),
            ColliderShape2d::CapsuleY { half_height, radius } => {
                capsule_wireframe_2d(half_height, radius, CIRCLE_SEGMENTS, false)
            }
            ColliderShape2d::CapsuleX { half_height, radius } => {
                capsule_wireframe_2d(half_height, radius, CIRCLE_SEGMENTS, true)
            }
        };
        transform_vertices_2d(&mut shape_verts, translation, angle);
        all_verts.extend_from_slice(&shape_verts);
    }

    if all_verts.is_empty() {
        return;
    }

    let vertex_buffer = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("2d debug wireframe vertices"),
            contents: bytemuck::cast_slice(&all_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("2d debug wireframe pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
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

        pass.set_pipeline(&debug_renderer.pipeline);
        pass.set_bind_group(0, &renderer.camera_bind_group, &[]);
        pass.set_bind_group(1, &debug_renderer.color_bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..all_verts.len() as u32, 0..1);
    }
}
