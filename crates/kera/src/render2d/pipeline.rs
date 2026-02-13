//! # Pipeline — The Full GPU Configuration for Drawing
//!
//! A *render pipeline* is a wgpu object that bundles everything the GPU needs
//! to know about how to draw: which shaders to run, how vertices are laid out,
//! how colors are blended, and what kind of primitives (triangles) to produce.
//! Think of it as a frozen configuration — once created, you just bind it
//! before issuing draw calls and the GPU knows what to do.
//!
//! ## What Each Piece Does
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │ RenderPipeline                                              │
//! │                                                             │
//! │  Shader module ─── vs_main + fs_main from shader.wgsl      │
//! │                                                             │
//! │  Vertex layout ─── SpriteVertex { pos, uv, color }         │
//! │                    tells the GPU how to read the buffer     │
//! │                                                             │
//! │  Bind group layouts                                         │
//! │    group 0: camera uniform (mat4x4, vertex-only)            │
//! │    group 1: texture + sampler (fragment-only)               │
//! │                                                             │
//! │  Blend state ─── ALPHA_BLENDING                             │
//! │    final = src.rgb × src.a + dst.rgb × (1 - src.a)         │
//! │    enables semi-transparent sprites                         │
//! │                                                             │
//! │  Primitive ─── TriangleList, no culling                     │
//! │    2D sprites are double-sided; backface culling off        │
//! │                                                             │
//! │  Depth/stencil ─── None                                     │
//! │    painter's algorithm handles ordering via Z-sort          │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Why Alpha Blending
//!
//! Sprites often have transparent regions (PNG alpha) or semi-transparent
//! tinting. Alpha blending composites the sprite's color over whatever was
//! already drawn to the framebuffer. The standard formula
//! `src × src_alpha + dst × (1 - src_alpha)` is exactly
//! `wgpu::BlendState::ALPHA_BLENDING`. Without it, transparent pixels would
//! render as black (or whatever the clear color is).
//!
//! ## Why No Depth Buffer
//!
//! A depth buffer lets the GPU skip fragments that are "behind" already-drawn
//! pixels. This works great for opaque 3D geometry, but breaks with
//! transparency: if a semi-transparent sprite writes to the depth buffer, it
//! blocks sprites behind it from blending correctly. Since we Z-sort on the
//! CPU and draw back-to-front, every sprite blends correctly without a depth
//! test.
//!
//! ## Lazy Initialization
//!
//! The [`SpriteRenderer`] is created on the first frame that actually renders,
//! not at startup. This is because the GPU context (`wgpu::Device`,
//! `wgpu::Surface`, etc.) is only available after the window is created and
//! the adapter is selected — which happens asynchronously. Deferring creation
//! to first-use keeps startup simple and avoids Option-wrapping everything.
//!
//! ## Comparison
//!
//! - **Raw wgpu examples**: Same structure — create pipeline once, bind before
//!   draw. We just wrap it in a struct for convenience.
//! - **Bevy**: Pipelines are managed by a `RenderPipelineCache` that
//!   deduplicates and hot-reloads shaders. Much more infrastructure.
//! - **OpenGL**: The equivalent is a combination of shader program, VAO state,
//!   and `glBlendFunc` calls. wgpu bundles all of this into one immutable
//!   object, which is easier to reason about.

use std::path::PathBuf;

use wgpu::util::DeviceExt;

use super::vertex::{CameraUniform, SpriteVertex};
use crate::render::GpuContext;

/// GPU resources for the 2D sprite renderer. Lazy-initialized on first frame.
pub(crate) struct SpriteRenderer {
    pub pipeline: wgpu::RenderPipeline,
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub sampler: wgpu::Sampler,
    pub vertex_buffer: Option<wgpu::Buffer>,
    pub index_buffer: Option<wgpu::Buffer>,
    /// Path to the shader source file on disk (for hot-reload). `None` if the
    /// source file doesn't exist at runtime (release builds without source).
    pub shader_path: Option<PathBuf>,
}

impl SpriteRenderer {
    /// Create the sprite renderer from the current GPU context.
    pub fn new(gpu: &GpuContext) -> Self {
        let device = &gpu.device;

        // Shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("shader.wgsl").into(),
            ),
        });

        // Bind group layout 0: camera uniform
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // Bind group layout 1: texture + sampler
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // Pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite pipeline layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Render pipeline — alpha blending enabled for sprites
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprite pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[SpriteVertex::LAYOUT],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.surface_format(),
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // 2D sprites are double-sided
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Camera uniform buffer (identity initially)
        let camera_uniform = CameraUniform {
            view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
        };
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera uniform buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Camera bind group
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // Shared sampler for all sprite textures
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sprite sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Locate shader source on disk for hot-reload (dev builds only).
        let shader_path = {
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("src")
                .join("render2d")
                .join("shader.wgsl");
            if p.exists() { Some(p) } else { None }
        };

        Self {
            pipeline,
            camera_bind_group_layout,
            texture_bind_group_layout,
            camera_buffer,
            camera_bind_group,
            sampler,
            vertex_buffer: None,
            index_buffer: None,
            shader_path,
        }
    }

    /// Build a new render pipeline from a shader module (hot-reload).
    ///
    /// Reuses the existing bind group layouts. Returns the candidate pipeline
    /// **without** swapping it in — the caller must check the error scope first
    /// and only assign to `self.pipeline` if valid.
    pub fn build_pipeline(&self, gpu: &GpuContext, shader: &wgpu::ShaderModule) -> wgpu::RenderPipeline {
        let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite pipeline layout (hot-reload)"),
            bind_group_layouts: &[&self.camera_bind_group_layout, &self.texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprite pipeline (hot-reload)"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[SpriteVertex::LAYOUT],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.surface_format(),
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }
}
