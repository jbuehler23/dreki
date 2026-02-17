//! # Collect — Query the ECS and Build Draw Commands
//!
//! Each frame, we need to gather everything visible in the scene: camera
//! parameters, light data, and all mesh entities with their transforms and
//! materials. This module converts ECS data into GPU-ready uniform structs.
//!
//! ## Draw Call Structure
//!
//! Each draw call needs:
//! - A mesh (vertex + index buffers)
//! - A material (bind group 2: uniform + texture)
//! - A model transform (bind group 3: dynamic offset into model buffer)
//!
//! Draw calls are sorted by material to minimize bind group 2 changes.
//! Within the same material, draw calls are ordered by mesh handle (though
//! this doesn't save GPU state changes in our current design, it groups
//! similar geometry for cache friendliness).
//!
//! ## Camera and Lights
//!
//! The camera view-projection matrix and all light data are collected first
//! and written to their respective uniform buffers (groups 0 and 1). These
//! are bound once and don't change during the frame.
//!
//! ## Comparison
//!
//! - **Bevy**: Uses a sophisticated `RenderPhase` system with sort keys,
//!   batching, and parallel extraction from the main world to a render world.
//! - **Our approach**: Simple serial collection into Vec, sort, done.

use crate::ecs::World;
use crate::ecs::hierarchy::GlobalTransform;

use super::mesh::MeshHandle;
use super::texture::TextureHandle3d;
use super::vertex::{
    CameraUniform3d, LightUniform, MaterialUniform, ModelUniform, PointLightData, MAX_POINT_LIGHTS,
};
use super::shape::Shape3d;
use super::{AmbientLight, Camera3d, DirectionalLight, Material, Mesh3d, PointLight};

/// A single draw command ready for the render pass.
pub(crate) struct DrawCall {
    pub mesh: MeshHandle,
    pub material_uniform: MaterialUniform,
    pub base_color_texture: Option<TextureHandle3d>,
    pub model_uniform: ModelUniform,
}

/// Collect the camera VP matrix and position from the scene.
pub(crate) fn collect_camera(
    world: &mut World,
    surface_size: (u32, u32),
) -> CameraUniform3d {
    let (width, height) = surface_size;
    let aspect = width as f32 / height.max(1) as f32;

    let mut camera_uniform = CameraUniform3d {
        view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
        camera_pos: [0.0; 3],
        _padding: 0.0,
    };

    world.query_single::<(&GlobalTransform, &Camera3d), Camera3d>(|_entity, (gt, cam)| {
        let projection = glam::Mat4::perspective_rh(
            cam.fov_y.to_radians(),
            aspect,
            cam.near,
            cam.far,
        );
        let view = gt.matrix.inverse();
        camera_uniform.view_proj = (projection * view).to_cols_array_2d();
        camera_uniform.camera_pos = gt.matrix.col(3).truncate().to_array();
    });

    camera_uniform
}

/// Collect all light data into a single uniform struct.
pub(crate) fn collect_lights(world: &mut World) -> LightUniform {
    let mut uniform = LightUniform {
        dir_direction: [0.0, -1.0, 0.0],
        dir_intensity: 0.0,
        dir_color: [1.0, 1.0, 1.0],
        _pad0: 0.0,
        ambient_color: [1.0, 1.0, 1.0],
        ambient_intensity: 0.1,
        point_lights: [bytemuck::Zeroable::zeroed(); MAX_POINT_LIGHTS],
        point_light_count: 0,
        _pad1: [0; 3],
    };

    // Directional light (use first found)
    let mut found_dir = false;
    world.query::<(&DirectionalLight,)>(|_entity, (light,)| {
        if !found_dir {
            uniform.dir_direction = light.direction.to_array();
            uniform.dir_color = light.color;
            uniform.dir_intensity = light.intensity;
            found_dir = true;
        }
    });

    // Ambient light (resource)
    if let Some(ambient) = world.get_resource::<AmbientLight>() {
        uniform.ambient_color = ambient.color;
        uniform.ambient_intensity = ambient.intensity;
    }

    // Point lights (up to MAX_POINT_LIGHTS)
    let mut count = 0u32;
    world.query::<(&GlobalTransform, &PointLight)>(|_entity, (gt, light)| {
        if (count as usize) < MAX_POINT_LIGHTS {
            uniform.point_lights[count as usize] = PointLightData {
                position: gt.matrix.col(3).truncate().to_array(),
                radius: light.radius,
                color: light.color,
                intensity: light.intensity,
            };
            count += 1;
        }
    });
    uniform.point_light_count = count;

    uniform
}

/// Collect all mesh entities into draw calls, sorted by material.
pub(crate) fn collect_draw_calls(world: &mut World) -> Vec<DrawCall> {
    let mut calls = Vec::new();

    world.query::<(&GlobalTransform, &Mesh3d, &Material)>(|_entity, (gt, mesh3d, material)| {
        let model = gt.matrix;
        // Normal matrix: inverse transpose of upper 3x3, stored as mat4x4.
        // For uniform scale, this equals the model matrix itself.
        // For non-uniform scale, we need the proper inverse transpose.
        let normal_matrix = model.inverse().transpose();

        let mat_uniform = MaterialUniform {
            base_color: material.base_color,
            metallic: material.metallic,
            roughness: material.roughness,
            _pad0: [0.0; 2],
            emissive: material.emissive,
            _pad1: 0.0,
        };

        let model_uniform = ModelUniform {
            model: model.to_cols_array_2d(),
            normal_matrix: normal_matrix.to_cols_array_2d(),
        };

        calls.push(DrawCall {
            mesh: mesh3d.mesh,
            material_uniform: mat_uniform,
            base_color_texture: material.base_color_texture,
            model_uniform,
        });
    });

    // Collect Shape3d entities (single-component alternative to Mesh3d + Material).
    world.query::<(&GlobalTransform, &Shape3d)>(|_entity, (gt, shape)| {
        let shape_scale = shape.shape_scale();
        let model = gt.matrix * glam::Mat4::from_scale(shape_scale);
        let normal_matrix = model.inverse().transpose();

        let mat_uniform = MaterialUniform {
            base_color: shape.base_color,
            metallic: shape.metallic,
            roughness: shape.roughness,
            _pad0: [0.0; 2],
            emissive: [0.0, 0.0, 0.0],
            _pad1: 0.0,
        };

        let model_uniform = ModelUniform {
            model: model.to_cols_array_2d(),
            normal_matrix: normal_matrix.to_cols_array_2d(),
        };

        calls.push(DrawCall {
            mesh: shape.mesh_handle(),
            material_uniform: mat_uniform,
            base_color_texture: None,
            model_uniform,
        });
    });

    // Sort by material parameters to minimize bind group 2 changes.
    // We use a simple key: (texture handle, metallic bits, roughness bits).
    calls.sort_by(|a, b| {
        let key_a = material_sort_key(&a.material_uniform, a.base_color_texture);
        let key_b = material_sort_key(&b.material_uniform, b.base_color_texture);
        key_a.cmp(&key_b)
    });

    calls
}

/// Generate a sort key for a material to group similar materials together.
fn material_sort_key(mat: &MaterialUniform, tex: Option<TextureHandle3d>) -> u64 {
    let tex_id = match tex {
        Some(h) => h.0 as u32,
        None => u32::MAX,
    };
    let metallic_bits = mat.metallic.to_bits();
    let roughness_bits = mat.roughness.to_bits();
    // Pack into u64: texture in high 32 bits, material params in low 32
    ((tex_id as u64) << 32) | ((metallic_bits ^ roughness_bits) as u64)
}
