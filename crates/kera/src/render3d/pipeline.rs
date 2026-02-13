//! # Pipeline — 3D Render Pipeline and GPU Resources
//!
//! The 3D pipeline is more complex than the 2D one because it needs:
//!
//! - **Depth buffer**: Tests each fragment against previously-drawn geometry.
//!   Unlike 2D (painter's algorithm), 3D meshes can interpenetrate and have
//!   complex overlapping shapes — a depth buffer handles this correctly.
//!
//! - **Four bind group layouts**: Camera, lights, material, and model
//!   transform. See [`vertex`](super::vertex) for the uniform buffer layouts.
//!
//! - **Backface culling**: Triangles facing away from the camera are skipped.
//!   This halves the fragment workload for closed meshes (cubes, spheres).
//!   2D sprites are double-sided, but 3D meshes have a clear "inside" and
//!   "outside."
//!
//! - **Dynamic uniform buffer for models**: Group 3 uses `has_dynamic_offset`
//!   so a single large buffer can hold all per-object data. Each `draw_indexed`
//!   call passes a byte offset into this buffer.
//!
//! ## Depth Buffer
//!
//! The depth buffer (or Z-buffer) is a texture the same size as the screen
//! where each pixel stores the depth of the closest fragment drawn so far.
//! Before writing a new fragment, the GPU compares its depth to the stored
//! value. If it's farther, the fragment is discarded — it's hidden behind
//! something already drawn.
//!
//! We use `Depth32Float` format: 32-bit floating-point depth values. This
//! provides high precision across the entire depth range and is universally
//! supported. The depth texture must be recreated whenever the window resizes.
//!
//! ## Comparison
//!
//! - **Bevy**: Uses a `RenderPipelineCache` with hot-reloading, specialization
//!   constants, and multi-pass rendering (shadow pass, main pass, etc.).
//! - **wgpu examples**: Similar structure but without the bind group split by
//!   change frequency — typically one or two bind groups.

use std::path::PathBuf;

use wgpu::util::DeviceExt;

use super::vertex::{
    CameraUniform3d, LightUniform, MeshVertex, ModelUniform,
};
use crate::render::GpuContext;

/// Depth texture format used by the 3D renderer.
pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// All GPU resources for the 3D mesh renderer. Lazy-initialized on first frame.
pub(crate) struct MeshRenderer {
    pub pipeline: wgpu::RenderPipeline,

    // Bind group layouts (needed to create per-frame bind groups and hot-reload)
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    pub light_bind_group_layout: wgpu::BindGroupLayout,
    pub material_bind_group_layout: wgpu::BindGroupLayout,
    pub model_bind_group_layout: wgpu::BindGroupLayout,

    // Per-frame buffers and bind groups (camera + lights)
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub light_buffer: wgpu::Buffer,
    pub light_bind_group: wgpu::BindGroup,

    // Shared sampler for all 3D textures
    pub sampler: wgpu::Sampler,

    // Depth buffer (recreated on resize)
    pub depth_texture: wgpu::TextureView,
    pub depth_size: (u32, u32),

    // Dynamic model uniform buffer (resized as needed)
    pub model_buffer: wgpu::Buffer,
    pub model_bind_group: wgpu::BindGroup,
    pub model_buffer_capacity: usize, // number of ModelUniform slots

    /// Path to the shader source file on disk (for hot-reload).
    pub shader_path: Option<PathBuf>,
}

impl MeshRenderer {
    /// Create the 3D renderer from the current GPU context.
    pub fn new(gpu: &GpuContext) -> Self {
        let device = &gpu.device;

        // ── Shader ──────────────────────────────────────────────────────
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pbr shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        // ── Bind group layout 0: Camera (per frame) ────────────────────
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("3d camera layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // ── Bind group layout 1: Lights (per frame) ────────────────────
        let light_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("3d light layout"),
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

        // ── Bind group layout 2: Material (per material) ───────────────
        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("3d material layout"),
                entries: &[
                    // MaterialUniform
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // base_color_texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // ── Bind group layout 3: Model (per object, dynamic offset) ────
        let model_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("3d model layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<ModelUniform>() as u64,
                        ),
                    },
                    count: None,
                }],
            });

        // ── Pipeline layout ─────────────────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("3d pipeline layout"),
            bind_group_layouts: &[
                &camera_bind_group_layout,
                &light_bind_group_layout,
                &material_bind_group_layout,
                &model_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        // ── Render pipeline ─────────────────────────────────────────────
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("3d pbr pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[MeshVertex::LAYOUT],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.surface_format(),
                    blend: None, // opaque only
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ── Camera buffer + bind group ──────────────────────────────────
        let camera_uniform = CameraUniform3d {
            view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            camera_pos: [0.0; 3],
            _padding: 0.0,
        };
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("3d camera buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("3d camera bind group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // ── Light buffer + bind group ───────────────────────────────────
        let light_uniform = LightUniform {
            dir_direction: [0.0, -1.0, 0.0],
            dir_intensity: 0.0,
            dir_color: [1.0, 1.0, 1.0],
            _pad0: 0.0,
            ambient_color: [1.0, 1.0, 1.0],
            ambient_intensity: 0.1,
            point_lights: [bytemuck::Zeroable::zeroed(); 8],
            point_light_count: 0,
            _pad1: [0; 3],
        };
        let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("3d light buffer"),
            contents: bytemuck::cast_slice(&[light_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let light_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("3d light bind group"),
            layout: &light_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_buffer.as_entire_binding(),
            }],
        });

        // ── Shared sampler ──────────────────────────────────────────────
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("3d sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── Depth texture ───────────────────────────────────────────────
        let (w, h) = gpu.surface_size();
        let depth_texture = create_depth_texture(device, w, h);

        // ── Dynamic model buffer ────────────────────────────────────────
        let initial_capacity = 64;
        let (model_buffer, model_bind_group) =
            create_model_buffer(device, &model_bind_group_layout, initial_capacity);

        // Locate shader source on disk for hot-reload (dev builds only).
        let shader_path = {
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("src")
                .join("render3d")
                .join("shader.wgsl");
            if p.exists() { Some(p) } else { None }
        };

        Self {
            pipeline,
            camera_bind_group_layout,
            light_bind_group_layout,
            material_bind_group_layout,
            model_bind_group_layout,
            camera_buffer,
            camera_bind_group,
            light_buffer,
            light_bind_group,
            sampler,
            depth_texture,
            depth_size: (w, h),
            model_buffer,
            model_bind_group,
            model_buffer_capacity: initial_capacity,
            shader_path,
        }
    }

    /// Recreate the depth texture if the surface size changed.
    pub fn resize_depth_if_needed(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if (width, height) != self.depth_size && width > 0 && height > 0 {
            self.depth_texture = create_depth_texture(device, width, height);
            self.depth_size = (width, height);
        }
    }

    /// Ensure the dynamic model buffer can hold `count` entries.
    /// Recreates if needed. Returns the aligned stride in bytes.
    pub fn ensure_model_capacity(
        &mut self,
        device: &wgpu::Device,
        count: usize,
    ) -> u32 {
        let align = device.limits().min_uniform_buffer_offset_alignment as usize;
        let stride = align_up(std::mem::size_of::<ModelUniform>(), align);

        if count > self.model_buffer_capacity {
            let new_cap = count.next_power_of_two();
            let (buffer, bind_group) =
                create_model_buffer(device, &self.model_bind_group_layout, new_cap);
            self.model_buffer = buffer;
            self.model_bind_group = bind_group;
            self.model_buffer_capacity = new_cap;
        }

        stride as u32
    }

    /// Build a new render pipeline from a shader module (hot-reload).
    ///
    /// Reuses the existing bind group layouts. Returns the candidate pipeline
    /// **without** swapping it in — the caller must check the error scope first
    /// and only assign to `self.pipeline` if valid.
    pub fn build_pipeline(&self, gpu: &GpuContext, shader: &wgpu::ShaderModule) -> wgpu::RenderPipeline {
        let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("3d pipeline layout (hot-reload)"),
            bind_group_layouts: &[
                &self.camera_bind_group_layout,
                &self.light_bind_group_layout,
                &self.material_bind_group_layout,
                &self.model_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("3d pbr pipeline (hot-reload)"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[MeshVertex::LAYOUT],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.surface_format(),
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }
}

/// Create a depth texture at the given dimensions.
fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("3d depth texture"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Create a dynamic model uniform buffer with the given capacity.
fn create_model_buffer(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    capacity: usize,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let align = device.limits().min_uniform_buffer_offset_alignment as usize;
    let stride = align_up(std::mem::size_of::<ModelUniform>(), align);
    let size = (stride * capacity) as u64;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("3d model dynamic buffer"),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("3d model bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: wgpu::BufferSize::new(std::mem::size_of::<ModelUniform>() as u64),
            }),
        }],
    });

    (buffer, bind_group)
}

/// Round `value` up to the next multiple of `align`.
fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}
