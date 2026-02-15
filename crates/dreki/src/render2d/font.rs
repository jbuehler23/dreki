//! # Font — TTF/OTF Rasterization and Text Rendering
//!
//! Uses [fontdue](https://docs.rs/fontdue) to rasterize TrueType/OpenType fonts
//! into a texture atlas. Each glyph becomes a set of white pixels with varying
//! alpha (`[255, 255, 255, coverage]`). The existing sprite shader multiplies
//! `texture_sample × tint_color`, so `white × color = color` with correct alpha.
//!
//! ## Atlas Packing
//!
//! At load time, ASCII 32–126 (95 printable characters) are rasterized at the
//! requested pixel size and packed row-by-row into a 512×512 RGBA texture with
//! 1px padding between glyphs. The atlas is uploaded to the `TextureStore` like
//! any other texture, so glyph quads flow through the same batching pipeline as
//! sprites.
//!
//! ## Text Component
//!
//! A `Text` component paired with a `Transform` spawns one quad per visible
//! glyph. The `batch.rs` module queries `(Transform, Text)` alongside
//! `(Transform, Sprite)` and emits glyph quads into the same vertex/index
//! buffers. All glyphs from the same font share a single atlas texture, so
//! an entire text string is typically one draw call (or merged with adjacent
//! sprites using the same atlas).

use wgpu::util::DeviceExt;

use crate::ecs::World;
use crate::render::GpuContext;

use super::pipeline::SpriteRenderer;
use super::texture::{TextureHandle, TextureStore};
use super::Color;

/// Handle to a loaded font in the [`FontStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontHandle(pub(crate) usize);

/// A text component. Pair with [`Transform`](crate::math::Transform) to position.
///
/// Each character is rendered as a textured quad using the font's glyph atlas.
#[derive(Debug, Clone)]
pub struct Text {
    /// The string to render.
    pub content: String,
    /// Which font to use.
    pub font: FontHandle,
    /// Tint color (multiplied with the white atlas glyphs).
    pub color: Color,
}

impl Text {
    /// Create a new text component with the given content and font.
    pub fn new(content: &str, font: FontHandle) -> Self {
        Self {
            content: content.to_owned(),
            font,
            color: Color::WHITE,
        }
    }

    /// Set the text color (builder pattern).
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

/// Per-glyph metrics and UV coordinates in the atlas.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GlyphInfo {
    /// UV rectangle in the atlas (normalized 0..1).
    pub u_min: f32,
    pub v_min: f32,
    pub u_max: f32,
    pub v_max: f32,
    /// Horizontal advance to next glyph (in pixels).
    pub advance: f32,
    /// Horizontal offset from cursor to glyph left edge.
    pub offset_x: f32,
    /// Vertical offset: distance from baseline to glyph top (Y-up).
    pub offset_y: f32,
    /// Glyph pixel dimensions.
    pub width: f32,
    pub height: f32,
}

/// Internal entry for one loaded font.
pub(crate) struct FontEntry {
    /// Glyph info indexed by `(char as u32 - 32)` for ASCII 32–126.
    pub glyphs: Vec<Option<GlyphInfo>>,
    /// Atlas texture handle in the TextureStore.
    pub atlas_handle: TextureHandle,
    /// Line height in pixels (for newline advancement).
    pub line_height: f32,
}

impl FontEntry {
    /// Look up glyph info for a character. Returns `None` for unsupported chars.
    pub fn glyph(&self, ch: char) -> Option<&GlyphInfo> {
        let idx = ch as u32;
        if idx < 32 || idx > 126 {
            return None;
        }
        self.glyphs[(idx - 32) as usize].as_ref()
    }
}

/// Resource storing all loaded fonts.
pub(crate) struct FontStore {
    entries: Vec<FontEntry>,
}

impl FontStore {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn get(&self, handle: FontHandle) -> &FontEntry {
        &self.entries[handle.0]
    }

    fn push(&mut self, entry: FontEntry) -> FontHandle {
        let handle = FontHandle(self.entries.len());
        self.entries.push(entry);
        handle
    }
}

const ATLAS_SIZE: u32 = 512;
const GLYPH_PADDING: u32 = 1;

/// Load a TTF/OTF font from disk at the given pixel size.
///
/// Rasterizes ASCII 32–126, packs into a 512×512 atlas, uploads as a texture.
/// Returns a [`FontHandle`] for use in [`Text`] components.
pub fn load_font(world: &mut World, path: &str, size: f32) -> FontHandle {
    // Ensure TextureStore + SpriteRenderer exist
    if !world.has_resource::<TextureStore>() {
        let gpu = world.resource::<GpuContext>();
        let renderer = SpriteRenderer::new(gpu);
        let store = TextureStore::new(gpu, &renderer);
        world.insert_resource(renderer);
        world.insert_resource(store);
    }

    if !world.has_resource::<FontStore>() {
        world.insert_resource(FontStore::new());
    }

    // Read font file
    let font_data = std::fs::read(path)
        .unwrap_or_else(|e| panic!("Failed to read font '{}': {}", path, e));

    let font = fontdue::Font::from_bytes(font_data, fontdue::FontSettings {
        scale: size,
        ..Default::default()
    })
    .unwrap_or_else(|e| panic!("Failed to parse font '{}': {}", path, e));

    // Rasterize ASCII 32–126
    let mut rasterized: Vec<(char, fontdue::Metrics, Vec<u8>)> = Vec::with_capacity(95);
    for code in 32u8..=126 {
        let ch = code as char;
        let (metrics, bitmap) = font.rasterize(ch, size);
        rasterized.push((ch, metrics, bitmap));
    }

    // Pack into atlas (row-based)
    let atlas_w = ATLAS_SIZE;
    let atlas_h = ATLAS_SIZE;
    let mut atlas_rgba = vec![0u8; (atlas_w * atlas_h * 4) as usize];
    let mut cursor_x: u32 = GLYPH_PADDING;
    let mut cursor_y: u32 = GLYPH_PADDING;
    let mut row_height: u32 = 0;

    let mut glyphs: Vec<Option<GlyphInfo>> = Vec::with_capacity(95);

    let line_height = size * 1.2;

    for &(ch, ref metrics, ref bitmap) in &rasterized {
        let gw = metrics.width as u32;
        let gh = metrics.height as u32;

        // Space and other zero-size glyphs
        if gw == 0 || gh == 0 {
            glyphs.push(Some(GlyphInfo {
                u_min: 0.0,
                v_min: 0.0,
                u_max: 0.0,
                v_max: 0.0,
                advance: metrics.advance_width,
                offset_x: 0.0,
                offset_y: 0.0,
                width: 0.0,
                height: 0.0,
            }));
            continue;
        }

        // Wrap to next row if needed
        if cursor_x + gw + GLYPH_PADDING > atlas_w {
            cursor_x = GLYPH_PADDING;
            cursor_y += row_height + GLYPH_PADDING;
            row_height = 0;
        }

        if cursor_y + gh + GLYPH_PADDING > atlas_h {
            log::warn!(
                "Font atlas overflow at char '{}' (U+{:04X}) — atlas too small",
                ch, ch as u32
            );
            glyphs.push(None);
            continue;
        }

        // Copy glyph bitmap into atlas as RGBA [255, 255, 255, alpha]
        for gy in 0..gh {
            for gx in 0..gw {
                let src_idx = (gy * gw + gx) as usize;
                let dst_x = cursor_x + gx;
                let dst_y = cursor_y + gy;
                let dst_idx = ((dst_y * atlas_w + dst_x) * 4) as usize;
                let alpha = bitmap[src_idx];
                atlas_rgba[dst_idx] = 255;
                atlas_rgba[dst_idx + 1] = 255;
                atlas_rgba[dst_idx + 2] = 255;
                atlas_rgba[dst_idx + 3] = alpha;
            }
        }

        // Compute UV coordinates (normalized)
        let u_min = cursor_x as f32 / atlas_w as f32;
        let v_min = cursor_y as f32 / atlas_h as f32;
        let u_max = (cursor_x + gw) as f32 / atlas_w as f32;
        let v_max = (cursor_y + gh) as f32 / atlas_h as f32;

        // fontdue: ymin is the distance from the baseline to the bottom of the glyph
        // (positive = above baseline for most glyphs). We negate for Y-up coordinate
        // system where the glyph hangs below the baseline origin.
        let offset_x = metrics.xmin as f32;
        let offset_y = metrics.ymin as f32;

        glyphs.push(Some(GlyphInfo {
            u_min,
            v_min,
            u_max,
            v_max,
            advance: metrics.advance_width,
            offset_x,
            offset_y,
            width: gw as f32,
            height: gh as f32,
        }));

        cursor_x += gw + GLYPH_PADDING;
        row_height = row_height.max(gh);
    }

    // Upload atlas to TextureStore with a Linear sampler for smooth text
    let mut texture_store = world
        .resource_remove::<TextureStore>()
        .expect("TextureStore missing");
    let gpu = world.resource::<GpuContext>();
    let renderer = world.resource::<SpriteRenderer>();

    let atlas_handle = upload_font_atlas(
        gpu,
        renderer,
        &mut texture_store,
        &atlas_rgba,
        atlas_w,
        atlas_h,
    );

    let entry = FontEntry {
        glyphs,
        atlas_handle,
        line_height,
    };

    let mut font_store = world
        .resource_remove::<FontStore>()
        .expect("FontStore missing");
    let handle = font_store.push(entry);

    world.insert_resource(texture_store);
    world.insert_resource(font_store);

    handle
}

/// Upload the font atlas as a texture with a Linear filter sampler.
fn upload_font_atlas(
    gpu: &GpuContext,
    renderer: &SpriteRenderer,
    texture_store: &mut TextureStore,
    data: &[u8],
    width: u32,
    height: u32,
) -> TextureHandle {
    let texture = gpu.device.create_texture_with_data(
        &gpu.queue,
        &wgpu::TextureDescriptor {
            label: Some("font atlas"),
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

    // Use Linear filtering for smoother text at fractional scales
    let linear_sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("font atlas sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("font atlas bind group"),
        layout: &renderer.texture_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&linear_sampler),
            },
        ],
    });

    let handle = TextureHandle(texture_store.entries.len());
    texture_store.entries.push(super::texture::TextureEntry {
        bind_group,
        width,
        height,
    });

    handle
}
