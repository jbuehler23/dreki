//! Debug wireframe rendering for 3D physics colliders.
//!
//! Draws collider shapes as green line segments on top of the 3D scene.
//! Uses a separate LineList pipeline that reads the existing depth buffer
//! (LessEqual, no write) so wireframes are occluded by geometry in front.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::math::{Quat, Vec3};
use crate::physics3d::ColliderShape3d;
use crate::render::gpu::GpuContext;

use super::pipeline::{MeshRenderer, DEPTH_FORMAT};

// ── Public resource ─────────────────────────────────────────────────────

/// Insert this resource to enable debug collider wireframes in 3D.
/// Toggle `enabled` at runtime (e.g. with F1).
#[derive(Debug)]
pub struct DebugColliders3d {
    pub enabled: bool,
    pub color: [f32; 4],
}

impl Default for DebugColliders3d {
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
struct DebugVertex {
    position: [f32; 3],
}

impl DebugVertex {
    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<DebugVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x3,
        }],
    };
}

// ── Renderer ────────────────────────────────────────────────────────────

pub(crate) struct DebugWireframeRenderer {
    pipeline: wgpu::RenderPipeline,
    color_buffer: wgpu::Buffer,
    color_bind_group: wgpu::BindGroup,
}

impl DebugWireframeRenderer {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("3d debug wireframe shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("debug_wireframe.wgsl").into()),
        });

        // Color uniform bind group layout (group 1)
        let color_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3d debug color layout"),
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
            label: Some("3d debug wireframe pipeline layout"),
            bind_group_layouts: &[camera_bind_group_layout, &color_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("3d debug wireframe pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[DebugVertex::LAYOUT],
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let color_data: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
        let color_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("3d debug color buffer"),
            contents: bytemuck::cast_slice(&color_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let color_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("3d debug color bind group"),
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

fn cuboid_wireframe(hx: f32, hy: f32, hz: f32) -> Vec<DebugVertex> {
    let corners = [
        [-hx, -hy, -hz],
        [ hx, -hy, -hz],
        [ hx,  hy, -hz],
        [-hx,  hy, -hz],
        [-hx, -hy,  hz],
        [ hx, -hy,  hz],
        [ hx,  hy,  hz],
        [-hx,  hy,  hz],
    ];
    // 12 edges
    let edges: [(usize, usize); 12] = [
        (0, 1), (1, 2), (2, 3), (3, 0), // front face
        (4, 5), (5, 6), (6, 7), (7, 4), // back face
        (0, 4), (1, 5), (2, 6), (3, 7), // connecting edges
    ];
    let mut verts = Vec::with_capacity(24);
    for (a, b) in edges {
        verts.push(DebugVertex { position: corners[a] });
        verts.push(DebugVertex { position: corners[b] });
    }
    verts
}

fn ball_wireframe(radius: f32, segments: u32) -> Vec<DebugVertex> {
    let mut verts = Vec::with_capacity(segments as usize * 6);
    let step = std::f32::consts::TAU / segments as f32;

    // XY circle
    for i in 0..segments {
        let a = i as f32 * step;
        let b = (i + 1) as f32 * step;
        verts.push(DebugVertex { position: [radius * a.cos(), radius * a.sin(), 0.0] });
        verts.push(DebugVertex { position: [radius * b.cos(), radius * b.sin(), 0.0] });
    }
    // XZ circle
    for i in 0..segments {
        let a = i as f32 * step;
        let b = (i + 1) as f32 * step;
        verts.push(DebugVertex { position: [radius * a.cos(), 0.0, radius * a.sin()] });
        verts.push(DebugVertex { position: [radius * b.cos(), 0.0, radius * b.sin()] });
    }
    // YZ circle
    for i in 0..segments {
        let a = i as f32 * step;
        let b = (i + 1) as f32 * step;
        verts.push(DebugVertex { position: [0.0, radius * a.cos(), radius * a.sin()] });
        verts.push(DebugVertex { position: [0.0, radius * b.cos(), radius * b.sin()] });
    }
    verts
}

fn capsule_wireframe(half_height: f32, radius: f32, segments: u32, axis: Axis) -> Vec<DebugVertex> {
    let mut verts = Vec::new();
    let step = std::f32::consts::TAU / segments as f32;
    let half_step = std::f32::consts::PI / segments as f32;

    match axis {
        Axis::Y => {
            // Two circles at top and bottom
            for i in 0..segments {
                let a = i as f32 * step;
                let b = (i + 1) as f32 * step;
                // Top circle
                verts.push(DebugVertex { position: [radius * a.cos(), half_height, radius * a.sin()] });
                verts.push(DebugVertex { position: [radius * b.cos(), half_height, radius * b.sin()] });
                // Bottom circle
                verts.push(DebugVertex { position: [radius * a.cos(), -half_height, radius * a.sin()] });
                verts.push(DebugVertex { position: [radius * b.cos(), -half_height, radius * b.sin()] });
            }
            // 4 connecting lines
            verts.push(DebugVertex { position: [radius, half_height, 0.0] });
            verts.push(DebugVertex { position: [radius, -half_height, 0.0] });
            verts.push(DebugVertex { position: [-radius, half_height, 0.0] });
            verts.push(DebugVertex { position: [-radius, -half_height, 0.0] });
            verts.push(DebugVertex { position: [0.0, half_height, radius] });
            verts.push(DebugVertex { position: [0.0, -half_height, radius] });
            verts.push(DebugVertex { position: [0.0, half_height, -radius] });
            verts.push(DebugVertex { position: [0.0, -half_height, -radius] });
            // Top semicircle (XY)
            let half_seg = segments / 2;
            for i in 0..half_seg {
                let a = i as f32 * half_step;
                let b = (i + 1) as f32 * half_step;
                verts.push(DebugVertex { position: [radius * a.cos(), half_height + radius * a.sin(), 0.0] });
                verts.push(DebugVertex { position: [radius * b.cos(), half_height + radius * b.sin(), 0.0] });
            }
            // Bottom semicircle (XY)
            for i in 0..half_seg {
                let a = std::f32::consts::PI + i as f32 * half_step;
                let b = std::f32::consts::PI + (i + 1) as f32 * half_step;
                verts.push(DebugVertex { position: [radius * a.cos(), -half_height + radius * a.sin(), 0.0] });
                verts.push(DebugVertex { position: [radius * b.cos(), -half_height + radius * b.sin(), 0.0] });
            }
            // Top semicircle (ZY)
            for i in 0..half_seg {
                let a = i as f32 * half_step;
                let b = (i + 1) as f32 * half_step;
                verts.push(DebugVertex { position: [0.0, half_height + radius * a.sin(), radius * a.cos()] });
                verts.push(DebugVertex { position: [0.0, half_height + radius * b.sin(), radius * b.cos()] });
            }
            // Bottom semicircle (ZY)
            for i in 0..half_seg {
                let a = std::f32::consts::PI + i as f32 * half_step;
                let b = std::f32::consts::PI + (i + 1) as f32 * half_step;
                verts.push(DebugVertex { position: [0.0, -half_height + radius * a.sin(), radius * a.cos()] });
                verts.push(DebugVertex { position: [0.0, -half_height + radius * b.sin(), radius * b.cos()] });
            }
        }
        Axis::X => {
            // Two circles at left and right
            for i in 0..segments {
                let a = i as f32 * step;
                let b = (i + 1) as f32 * step;
                verts.push(DebugVertex { position: [half_height, radius * a.cos(), radius * a.sin()] });
                verts.push(DebugVertex { position: [half_height, radius * b.cos(), radius * b.sin()] });
                verts.push(DebugVertex { position: [-half_height, radius * a.cos(), radius * a.sin()] });
                verts.push(DebugVertex { position: [-half_height, radius * b.cos(), radius * b.sin()] });
            }
            // 4 connecting lines
            verts.push(DebugVertex { position: [half_height, radius, 0.0] });
            verts.push(DebugVertex { position: [-half_height, radius, 0.0] });
            verts.push(DebugVertex { position: [half_height, -radius, 0.0] });
            verts.push(DebugVertex { position: [-half_height, -radius, 0.0] });
            verts.push(DebugVertex { position: [half_height, 0.0, radius] });
            verts.push(DebugVertex { position: [-half_height, 0.0, radius] });
            verts.push(DebugVertex { position: [half_height, 0.0, -radius] });
            verts.push(DebugVertex { position: [-half_height, 0.0, -radius] });
            // Semicircles along X axis
            let half_seg = segments / 2;
            for i in 0..half_seg {
                let a = i as f32 * half_step;
                let b = (i + 1) as f32 * half_step;
                verts.push(DebugVertex { position: [half_height + radius * a.sin(), radius * a.cos(), 0.0] });
                verts.push(DebugVertex { position: [half_height + radius * b.sin(), radius * b.cos(), 0.0] });
            }
            for i in 0..half_seg {
                let a = std::f32::consts::PI + i as f32 * half_step;
                let b = std::f32::consts::PI + (i + 1) as f32 * half_step;
                verts.push(DebugVertex { position: [-half_height + radius * a.sin(), radius * a.cos(), 0.0] });
                verts.push(DebugVertex { position: [-half_height + radius * b.sin(), radius * b.cos(), 0.0] });
            }
        }
        Axis::Z => {
            // Two circles at front and back
            for i in 0..segments {
                let a = i as f32 * step;
                let b = (i + 1) as f32 * step;
                verts.push(DebugVertex { position: [radius * a.cos(), radius * a.sin(), half_height] });
                verts.push(DebugVertex { position: [radius * b.cos(), radius * b.sin(), half_height] });
                verts.push(DebugVertex { position: [radius * a.cos(), radius * a.sin(), -half_height] });
                verts.push(DebugVertex { position: [radius * b.cos(), radius * b.sin(), -half_height] });
            }
            // 4 connecting lines
            verts.push(DebugVertex { position: [radius, 0.0, half_height] });
            verts.push(DebugVertex { position: [radius, 0.0, -half_height] });
            verts.push(DebugVertex { position: [-radius, 0.0, half_height] });
            verts.push(DebugVertex { position: [-radius, 0.0, -half_height] });
            verts.push(DebugVertex { position: [0.0, radius, half_height] });
            verts.push(DebugVertex { position: [0.0, radius, -half_height] });
            verts.push(DebugVertex { position: [0.0, -radius, half_height] });
            verts.push(DebugVertex { position: [0.0, -radius, -half_height] });
            // Semicircles along Z axis
            let half_seg = segments / 2;
            for i in 0..half_seg {
                let a = i as f32 * half_step;
                let b = (i + 1) as f32 * half_step;
                verts.push(DebugVertex { position: [radius * a.cos(), 0.0, half_height + radius * a.sin()] });
                verts.push(DebugVertex { position: [radius * b.cos(), 0.0, half_height + radius * b.sin()] });
            }
            for i in 0..half_seg {
                let a = std::f32::consts::PI + i as f32 * half_step;
                let b = std::f32::consts::PI + (i + 1) as f32 * half_step;
                verts.push(DebugVertex { position: [radius * a.cos(), 0.0, -half_height + radius * a.sin()] });
                verts.push(DebugVertex { position: [radius * b.cos(), 0.0, -half_height + radius * b.sin()] });
            }
        }
    }
    verts
}

enum Axis {
    X,
    Y,
    Z,
}

fn transform_vertices(verts: &mut [DebugVertex], translation: Vec3, rotation: Quat) {
    for v in verts.iter_mut() {
        let p = Vec3::new(v.position[0], v.position[1], v.position[2]);
        let p = rotation * p + translation;
        v.position = [p.x, p.y, p.z];
    }
}

// ── Render entry point ──────────────────────────────────────────────────

const CIRCLE_SEGMENTS: u32 = 32;

pub(crate) fn render_debug_wireframes_3d(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    gpu: &GpuContext,
    renderer: &MeshRenderer,
    debug_renderer: &mut DebugWireframeRenderer,
    debug_config: &DebugColliders3d,
    poses: &[(Vec3, Quat, ColliderShape3d)],
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
    let mut all_verts: Vec<DebugVertex> = Vec::new();
    for &(translation, rotation, shape) in poses {
        let mut shape_verts = match shape {
            ColliderShape3d::Cuboid { hx, hy, hz } => cuboid_wireframe(hx, hy, hz),
            ColliderShape3d::Ball { radius } => ball_wireframe(radius, CIRCLE_SEGMENTS),
            ColliderShape3d::CapsuleY { half_height, radius } => {
                capsule_wireframe(half_height, radius, CIRCLE_SEGMENTS, Axis::Y)
            }
            ColliderShape3d::CapsuleX { half_height, radius } => {
                capsule_wireframe(half_height, radius, CIRCLE_SEGMENTS, Axis::X)
            }
            ColliderShape3d::CapsuleZ { half_height, radius } => {
                capsule_wireframe(half_height, radius, CIRCLE_SEGMENTS, Axis::Z)
            }
        };
        transform_vertices(&mut shape_verts, translation, rotation);
        all_verts.extend_from_slice(&shape_verts);
    }

    if all_verts.is_empty() {
        return;
    }

    let vertex_buffer = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("3d debug wireframe vertices"),
            contents: bytemuck::cast_slice(&all_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("3d debug wireframe pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &renderer.depth_texture,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
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
