//! # Texture — Image Data on the GPU
//!
//! A *texture* is an image that has been uploaded from disk (or generated) into
//! GPU memory. The GPU can then sample it during rendering — reading a color
//! value at a given UV coordinate. This module handles loading PNG/JPEG files,
//! uploading them as `wgpu::Texture` objects, and creating the bind groups that
//! let the shader access them.
//!
//! ## The Handle Pattern
//!
//! Users never hold a `wgpu::Texture` directly. Instead, [`load_texture`]
//! returns a [`TextureHandle`] — a lightweight index into the [`TextureStore`].
//! This has several benefits:
//!
//! - **Copyable**: `TextureHandle` is `Copy`, so it can live in components
//!   without lifetime headaches.
//! - **Indirection**: The store owns the GPU resources and can manage their
//!   lifetime. The handle is just a `usize` index.
//! - **Deduplication**: The store caches by file path, so loading the same
//!   image twice returns the same handle without a second GPU upload.
//!
//! ```text
//! TextureStore
//! ┌───────────────────────────────────────────────┐
//! │ entries: Vec<TextureEntry>                    │
//! │   [0] 1x1 white (default)   ◄── always here  │
//! │   [1] "player.png"                            │
//! │   [2] "tileset.png"                           │
//! │   ...                                         │
//! │                                               │
//! │ path_cache: HashMap<String, TextureHandle>    │
//! │   "player.png"  → Handle(1)                   │
//! │   "tileset.png" → Handle(2)                   │
//! └───────────────────────────────────────────────┘
//! ```
//!
//! ## The 1x1 White Default Texture
//!
//! Entry 0 is always a single white pixel. When a [`Sprite`](super::Sprite)
//! has no texture, the renderer binds this default texture. The fragment shader
//! samples it (always white) and multiplies by the sprite's tint color —
//! producing a solid-colored rectangle. This avoids a separate "untextured"
//! code path in the shader; every sprite goes through the same
//! `texture × tint` pipeline.
//!
//! ## Extract/Reinsert Pattern
//!
//! [`load_texture`] needs mutable access to the `TextureStore` (to add an
//! entry) and shared access to `GpuContext` and `SpriteRenderer` (to create
//! GPU resources). All three are resources in the [`World`](crate::ecs::World),
//! and Rust's borrow checker won't let you hold `&mut Store` and `&GpuContext`
//! at the same time from the same `World`. The solution: *remove* the store
//! from the world, do the work, then *reinsert* it. This is a common pattern
//! in single-world ECS designs without interior mutability.
//!
//! ## Comparison
//!
//! - **Bevy** (`AssetServer`): Loads textures asynchronously, returns a
//!   `Handle<Image>` backed by reference counting and a global asset store.
//!   Supports hot-reloading. Far more complex, but handles async and
//!   deduplication automatically.
//! - **wgpu examples**: Textures are loaded manually with `image` crate and
//!   uploaded via `create_texture_with_data`. No handle abstraction — the app
//!   holds the `wgpu::Texture` directly.
//! - **Macroquad**: `load_texture("path")` is an async function returning a
//!   `Texture2D` handle. Similar simplicity, but async-based.

use std::collections::HashMap;
use std::path::PathBuf;

use wgpu::util::DeviceExt;

use crate::asset::{AssetKind, AssetServer};
use crate::ecs::World;
use crate::render::GpuContext;

use super::pipeline::SpriteRenderer;

/// Handle to a loaded texture in the [`TextureStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub(crate) usize);

/// Internal entry for a loaded GPU texture.
pub(crate) struct TextureEntry {
    pub bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
}

/// Stores all loaded GPU textures and their bind groups.
pub(crate) struct TextureStore {
    pub entries: Vec<TextureEntry>,
    path_cache: HashMap<String, TextureHandle>,
}

impl TextureStore {
    /// Create a new store with a 1x1 white default texture at index 0.
    pub fn new(gpu: &GpuContext, renderer: &SpriteRenderer) -> Self {
        let device = &gpu.device;
        let queue = &gpu.queue;

        // 1x1 white pixel
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("white 1x1"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &[255u8, 255, 255, 255],
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("white 1x1 bind group"),
            layout: &renderer.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&renderer.sampler),
                },
            ],
        });

        let default_entry = TextureEntry {
            bind_group,
            width: 1,
            height: 1,
        };

        Self {
            entries: vec![default_entry],
            path_cache: HashMap::new(),
        }
    }

    /// The default 1x1 white texture handle.
    pub fn default_handle(&self) -> TextureHandle {
        TextureHandle(0)
    }

    /// Get the entry for a handle.
    pub fn get(&self, handle: TextureHandle) -> &TextureEntry {
        &self.entries[handle.0]
    }

    /// Replace the GPU data for an existing texture handle (hot-reload).
    ///
    /// Creates a new GPU texture and bind group from the given RGBA8 data,
    /// then swaps them into the entry at the handle's index. Any sprite
    /// referencing this handle will see the new texture next frame.
    pub fn reload_entry(
        &mut self,
        gpu: &GpuContext,
        renderer: &SpriteRenderer,
        handle: TextureHandle,
        width: u32,
        height: u32,
        data: &[u8],
    ) {
        let texture = gpu.device.create_texture_with_data(
            &gpu.queue,
            &wgpu::TextureDescriptor {
                label: Some("hot-reload texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            data,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hot-reload bind group"),
            layout: &renderer.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&renderer.sampler),
                },
            ],
        });

        self.entries[handle.0] = TextureEntry {
            bind_group,
            width,
            height,
        };
    }
}

/// Create a texture from raw RGBA8 pixel data and return a handle.
///
/// Uses the same extract/reinsert pattern as [`load_texture`].
pub fn create_texture_from_rgba(
    world: &mut World,
    label: &str,
    width: u32,
    height: u32,
    data: &[u8],
) -> TextureHandle {
    // Ensure TextureStore + SpriteRenderer exist.
    if !world.has_resource::<TextureStore>() {
        let gpu = world.resource::<GpuContext>();
        let renderer = SpriteRenderer::new(gpu);
        let store = TextureStore::new(gpu, &renderer);
        world.insert_resource(renderer);
        world.insert_resource(store);
    }

    let mut store = world
        .resource_remove::<TextureStore>()
        .expect("TextureStore not initialized");

    let gpu = world.resource::<GpuContext>();
    let renderer = world.resource::<SpriteRenderer>();

    let texture = gpu.device.create_texture_with_data(
        &gpu.queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        data,
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout: &renderer.texture_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&renderer.sampler),
            },
        ],
    });

    let handle = TextureHandle(store.entries.len());
    store.entries.push(TextureEntry {
        bind_group,
        width,
        height,
    });

    world.insert_resource(store);
    handle
}

/// Load a texture from disk and return a handle.
///
/// Uses the extract/reinsert pattern: temporarily removes `TextureStore` from
/// the world to avoid borrow conflicts with `GpuContext`.
///
/// The texture is cached by path — loading the same path twice returns the
/// same handle.
pub fn load_texture(world: &mut World, path: &str) -> TextureHandle {
    // Ensure TextureStore + SpriteRenderer exist (lazy init if GpuContext is ready).
    if !world.has_resource::<TextureStore>() {
        let gpu = world.resource::<GpuContext>();
        let renderer = SpriteRenderer::new(gpu);
        let store = TextureStore::new(gpu, &renderer);
        world.insert_resource(renderer);
        world.insert_resource(store);
    }

    // Check cache first (need to remove store to mutate it)
    let mut store = world
        .resource_remove::<TextureStore>()
        .expect("TextureStore not initialized — GpuContext is missing");

    if let Some(&handle) = store.path_cache.get(path) {
        world.insert_resource(store);
        return handle;
    }

    let gpu = world.resource::<GpuContext>();
    let renderer = world.resource::<SpriteRenderer>();

    // Load image from disk
    let img = image::open(path)
        .unwrap_or_else(|e| panic!("Failed to load texture '{}': {}", path, e))
        .to_rgba8();
    let (width, height) = img.dimensions();
    let data = img.into_raw();

    let texture = gpu.device.create_texture_with_data(
        &gpu.queue,
        &wgpu::TextureDescriptor {
            label: Some(path),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &data,
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(path),
        layout: &renderer.texture_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&renderer.sampler),
            },
        ],
    });

    let handle = TextureHandle(store.entries.len());
    store.entries.push(TextureEntry {
        bind_group,
        width,
        height,
    });
    store.path_cache.insert(path.to_owned(), handle);

    world.insert_resource(store);

    // Register this file for hot-reload watching.
    if let Some(server) = world.get_resource_mut::<AssetServer>() {
        server.watch(PathBuf::from(path), AssetKind::Texture2d(handle));
    }

    handle
}
