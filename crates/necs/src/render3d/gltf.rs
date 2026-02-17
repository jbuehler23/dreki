//! # glTF — Loading 3D Models
//!
//! [glTF 2.0](https://www.khronos.org/gltf/) is the industry-standard format
//! for real-time 3D assets. Think of it as "JPEG for 3D" — it encodes meshes,
//! materials, textures, and scene hierarchy in a compact, GPU-friendly format.
//!
//! ## Format Variants
//!
//! - **`.gltf`**: JSON file + separate `.bin` (mesh data) + image files.
//!   Human-readable but multiple files to manage.
//! - **`.glb`**: Single binary file containing everything. Preferred for
//!   distribution — one file, no path issues.
//!
//! ## What We Extract
//!
//! For each mesh primitive in the file:
//! - **Positions**: `POSITION` accessor → `MeshVertex.position`
//! - **Normals**: `NORMAL` accessor → `MeshVertex.normal`
//! - **UVs**: `TEXCOORD_0` accessor → `MeshVertex.uv` (default [0,0] if absent)
//! - **Indices**: Index accessor → `u32` index buffer
//!
//! For each material:
//! - **base_color_factor** → `Material.base_color`
//! - **metallic_factor** → `Material.metallic`
//! - **roughness_factor** → `Material.roughness`
//! - **emissive_factor** → `Material.emissive`
//! - **base_color_texture** → loaded into TextureStore3d if present
//!
//! ## What We Skip (For Now)
//!
//! - Animations, skins, morph targets
//! - Scene hierarchy (all meshes placed at origin)
//! - Normal maps, occlusion maps, emissive maps
//! - Multiple UV sets
//!
//! ## Comparison
//!
//! - **Bevy**: Full glTF loader with scene spawning, animation, skins,
//!   morph targets, and async loading.
//! - **three.js**: `GLTFLoader` returns a scene graph with all features.
//! - **Our approach**: Minimal extraction — just geometry and basic PBR
//!   materials. The caller spawns entities manually.

use crate::ecs::World;
use crate::render::GpuContext;

use super::mesh::MeshStore;
use super::texture::TextureStore3d;
use super::vertex::MeshVertex;
use super::{Material, MeshHandle};

/// Load a glTF/GLB file and return (MeshHandle, Material) pairs.
///
/// Uses the extract/reinsert pattern for MeshStore and TextureStore3d.
/// The caller is responsible for spawning entities with the returned data.
///
/// # Example
/// ```ignore
/// let parts = load_gltf(world, "assets/helmet.glb");
/// for (mesh_handle, material) in parts {
///     world.spawn((
///         Transform::default(),
///         Mesh3d { mesh: mesh_handle },
///         material,
///     ));
/// }
/// ```
pub fn load_gltf(world: &mut World, path: &str) -> Vec<(MeshHandle, Material)> {
    let mut mesh_store = world
        .resource_remove::<MeshStore>()
        .expect("MeshStore not initialized — render at least one frame first");
    let mut texture_store = world
        .resource_remove::<TextureStore3d>()
        .expect("TextureStore3d not initialized");
    let gpu = world.resource::<GpuContext>();

    let result = load_gltf_inner(gpu, &mut mesh_store, &mut texture_store, path);

    world.insert_resource(mesh_store);
    world.insert_resource(texture_store);
    result
}

fn load_gltf_inner(
    gpu: &GpuContext,
    mesh_store: &mut MeshStore,
    texture_store: &mut TextureStore3d,
    path: &str,
) -> Vec<(MeshHandle, Material)> {
    let (document, buffers, images) = gltf::import(path)
        .unwrap_or_else(|e| panic!("Failed to load glTF '{path}': {e}"));

    let mut results = Vec::new();

    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            // Positions (required)
            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .expect("glTF mesh missing POSITION attribute")
                .collect();

            // Normals (optional, default to +Y)
            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|iter| iter.collect())
                .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

            // UVs (optional, default to [0, 0])
            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|iter| iter.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

            // Build MeshVertex array
            let vertices: Vec<MeshVertex> = positions
                .iter()
                .enumerate()
                .map(|(i, pos)| MeshVertex {
                    position: *pos,
                    normal: normals[i],
                    uv: uvs[i],
                })
                .collect();

            // Indices (required for our pipeline)
            let indices: Vec<u32> = reader
                .read_indices()
                .expect("glTF mesh missing indices")
                .into_u32()
                .collect();

            let mesh_handle = mesh_store.upload(gpu, &vertices, &indices);

            // Extract material
            let material = {
                let pbr = primitive.material().pbr_metallic_roughness();
                let base_color_factor = pbr.base_color_factor();
                let metallic = pbr.metallic_factor();
                let roughness = pbr.roughness_factor();
                let emissive = primitive.material().emissive_factor();

                // Base color texture
                let base_color_texture = pbr.base_color_texture().and_then(|info| {
                    let tex = info.texture();
                    let source = tex.source();
                    let image = &images[source.index()];
                    match image.format {
                        gltf::image::Format::R8G8B8A8 => {
                            Some(texture_store.upload_rgba8(
                                gpu,
                                &format!("{path}:tex{}", source.index()),
                                image.width,
                                image.height,
                                &image.pixels,
                            ))
                        }
                        gltf::image::Format::R8G8B8 => {
                            // Convert RGB to RGBA
                            let mut rgba = Vec::with_capacity(image.pixels.len() / 3 * 4);
                            for chunk in image.pixels.chunks(3) {
                                rgba.extend_from_slice(chunk);
                                rgba.push(255);
                            }
                            Some(texture_store.upload_rgba8(
                                gpu,
                                &format!("{path}:tex{}", source.index()),
                                image.width,
                                image.height,
                                &rgba,
                            ))
                        }
                        _ => None, // Skip unsupported formats
                    }
                });

                Material {
                    base_color: base_color_factor,
                    base_color_texture,
                    metallic,
                    roughness,
                    emissive,
                }
            };

            results.push((mesh_handle, material));
        }
    }

    results
}
