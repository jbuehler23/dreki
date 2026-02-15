//! # Texture — Image Data for 3D Materials
//!
//! Same handle-based pattern as the 2D texture store, but adapted for 3D PBR
//! materials. The key difference: bind groups are not stored here. In the 3D
//! pipeline, textures are combined with material parameters into per-material
//! bind groups (group 2) during the draw phase. This module just stores the
//! GPU texture views.
//!
//! ## The 1x1 White Default
//!
//! Entry 0 is always a single white pixel, same as the 2D renderer. When a
//! [`Material`](super::Material) has no `base_color_texture`, the default
//! white texture is bound. The shader samples it (always white) and uses the
//! material's `base_color` field directly. This avoids branching in the shader.
//!
//! ## Comparison
//!
//! - **Bevy**: Uses an `AssetServer` with typed `Handle<Image>`, async loading,
//!   reference counting, and hot-reloading. Much more infrastructure.
//! - **three.js**: `TextureLoader` with callbacks, shared `Texture` objects.
//! - **Our approach**: Synchronous, index-based, with path deduplication.

use std::collections::HashMap;
use std::path::PathBuf;

use wgpu::util::DeviceExt;

use crate::asset::{AssetKind, AssetServer};
use crate::ecs::World;
use crate::render::GpuContext;

/// Handle to a loaded texture in the 3D [`TextureStore3d`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle3d(pub(crate) usize);

/// Internal entry for a loaded GPU texture.
pub(crate) struct TextureEntry3d {
    pub view: wgpu::TextureView,
    #[allow(dead_code)]
    pub width: u32,
    #[allow(dead_code)]
    pub height: u32,
}

/// Stores all loaded GPU textures for the 3D renderer.
pub(crate) struct TextureStore3d {
    pub entries: Vec<TextureEntry3d>,
    path_cache: HashMap<String, TextureHandle3d>,
}

impl TextureStore3d {
    /// Create a new store with a 1x1 white default texture at index 0.
    pub fn new(gpu: &GpuContext) -> Self {
        let texture = gpu.device.create_texture_with_data(
            &gpu.queue,
            &wgpu::TextureDescriptor {
                label: Some("3d white 1x1"),
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

        Self {
            entries: vec![TextureEntry3d {
                view,
                width: 1,
                height: 1,
            }],
            path_cache: HashMap::new(),
        }
    }

    /// The default 1x1 white texture handle.
    pub fn default_handle(&self) -> TextureHandle3d {
        TextureHandle3d(0)
    }

    /// Get the entry for a handle.
    pub fn get(&self, handle: TextureHandle3d) -> &TextureEntry3d {
        &self.entries[handle.0]
    }


    /// Upload a texture from raw RGBA8 data.
    pub fn upload_rgba8(
        &mut self,
        gpu: &GpuContext,
        label: &str,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> TextureHandle3d {
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
        let handle = TextureHandle3d(self.entries.len());
        self.entries.push(TextureEntry3d {
            view,
            width,
            height,
        });
        handle
    }

    /// Replace the GPU data for an existing texture handle (hot-reload).
    ///
    /// Creates a new GPU texture view from the given RGBA8 data and swaps it
    /// into the entry at the handle's index. Bind groups referencing this
    /// texture are recreated each frame anyway, so they'll pick up the new view.
    pub fn reload_entry(
        &mut self,
        gpu: &GpuContext,
        handle: TextureHandle3d,
        width: u32,
        height: u32,
        data: &[u8],
    ) {
        let texture = gpu.device.create_texture_with_data(
            &gpu.queue,
            &wgpu::TextureDescriptor {
                label: Some("3d hot-reload texture"),
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
        self.entries[handle.0] = TextureEntry3d {
            view,
            width,
            height,
        };
    }
}

/// Load a texture from disk for the 3D renderer.
///
/// Uses the extract/reinsert pattern to avoid borrow conflicts.
pub fn load_texture_3d(world: &mut World, path: &str) -> TextureHandle3d {
    let mut store = world
        .resource_remove::<TextureStore3d>()
        .expect("TextureStore3d not initialized — render at least one frame first");

    if let Some(&handle) = store.path_cache.get(path) {
        world.insert_resource(store);
        return handle;
    }

    let gpu = world.resource::<GpuContext>();

    let img = image::open(path)
        .unwrap_or_else(|e| panic!("Failed to load 3D texture '{path}': {e}"))
        .to_rgba8();
    let (width, height) = img.dimensions();
    let data = img.into_raw();

    let handle = store.upload_rgba8(gpu, path, width, height, &data);
    store.path_cache.insert(path.to_owned(), handle);

    world.insert_resource(store);

    // Register this file for hot-reload watching.
    if let Some(server) = world.get_resource_mut::<AssetServer>() {
        server.watch(PathBuf::from(path), AssetKind::Texture3d(handle));
    }

    handle
}
