// ============================================================================
// PBR Shader — Physically Based Rendering with Cook-Torrance BRDF
//
// This shader implements a simplified version of the metallic-roughness PBR
// model used by glTF 2.0, Unreal Engine, Unity, and Blender. The core idea:
// instead of tweaking arbitrary "shininess" and "specular color" values, we
// describe surfaces with two physically meaningful properties:
//
//   metallic (0–1): Is this a metal or a non-metal (dielectric)?
//     Metals reflect their base color. Dielectrics reflect white at ~4%.
//
//   roughness (0–1): How rough is the surface?
//     Rough surfaces scatter light broadly (diffuse look).
//     Smooth surfaces concentrate light into sharp highlights (mirror look).
//
// The rendering equation we approximate:
//
//   L_out = ∫ BRDF(ω_i, ω_o) × L_in(ω_i) × cos(θ_i) dω_i
//
// For real-time rendering, we replace the integral with a sum over discrete
// light sources, and the BRDF with the Cook-Torrance microfacet model:
//
//   BRDF = k_d × f_Lambert  +  k_s × f_CookTorrance
//
// Where:
//   f_Lambert = base_color / π                    (diffuse)
//   f_CookTorrance = D × F × G / (4 × NdotV × NdotL)  (specular)
//
// The three functions D, F, G model how microfacets on the surface
// distribute, reflect, and shadow light. See each function below for details.
// ============================================================================

// ── Bind Group 0: Camera (per frame) ────────────────────────────────────────

struct CameraUniform {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// ── Bind Group 1: Lights (per frame) ────────────────────────────────────────

struct PointLightData {
    position: vec3<f32>,
    radius: f32,
    color: vec3<f32>,
    intensity: f32,
};

struct LightUniform {
    // Directional light
    dir_direction: vec3<f32>,
    dir_intensity: f32,
    dir_color: vec3<f32>,
    _pad0: f32,
    // Ambient
    ambient_color: vec3<f32>,
    ambient_intensity: f32,
    // Point lights (fixed array of 8)
    point_lights: array<PointLightData, 8>,
    // Count (use individual u32 padding to match Rust [u32; 3] alignment)
    point_light_count: u32,
    _pad1a: u32,
    _pad1b: u32,
    _pad1c: u32,
};
@group(1) @binding(0)
var<uniform> lights: LightUniform;

// ── Bind Group 2: Material (per material) ───────────────────────────────────

struct MaterialUniform {
    base_color: vec4<f32>,
    metallic: f32,
    roughness: f32,
    _pad0: vec2<f32>,
    emissive: vec3<f32>,
    _pad1: f32,
};
@group(2) @binding(0)
var<uniform> material: MaterialUniform;
@group(2) @binding(1)
var base_color_texture: texture_2d<f32>;
@group(2) @binding(2)
var base_color_sampler: sampler;

// ── Bind Group 3: Model (per object, dynamic offset) ────────────────────────

struct ModelUniform {
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};
@group(3) @binding(0)
var<uniform> model: ModelUniform;

// ── Vertex Shader ───────────────────────────────────────────────────────────

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Transform position from local space → world space → clip space.
    let world_pos = model.model * vec4<f32>(in.position, 1.0);
    out.world_pos = world_pos.xyz;
    out.clip_position = camera.view_proj * world_pos;

    // Transform normal from local space → world space.
    // The normal matrix is the inverse transpose of the model matrix.
    // We use the upper 3x3 of the mat4x4 (the rest is padding).
    out.world_normal = normalize((model.normal_matrix * vec4<f32>(in.normal, 0.0)).xyz);

    out.uv = in.uv;
    return out;
}

// ── PBR Functions ───────────────────────────────────────────────────────────

const PI: f32 = 3.14159265359;

// D: Normal Distribution Function (GGX / Trowbridge-Reitz)
//
// Models the statistical distribution of microfacet orientations. Given a
// halfway vector H between the light and view directions, D tells us what
// fraction of microfacets are oriented to reflect light toward the camera.
//
// At roughness=0 (perfect mirror), D is a spike — only one direction reflects.
// At roughness=1, D is broad — microfacets point in many directions.
//
// Formula: D = α² / (π × ((N·H)² × (α² - 1) + 1)²)
//   where α = roughness²
fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom);
}

// F: Fresnel (Schlick approximation)
//
// Models how reflectivity changes with viewing angle. At grazing angles
// (looking along the surface), all surfaces become mirror-like — this is
// why even a calm lake reflects the sky near the horizon.
//
// F0 is the reflectance at normal incidence (looking straight at the surface):
//   - Dielectrics (plastic, wood): F0 ≈ 0.04 (4% reflection)
//   - Metals (gold, copper): F0 = base_color (colored reflection)
//
// Formula: F = F0 + (1 - F0) × (1 - cos θ)⁵
fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// G: Geometry Function (Smith's method with Schlick-GGX)
//
// Models self-shadowing of microfacets. Rough surfaces have tall microfacets
// that can block light from reaching (shadowing) or reflecting toward
// (masking) neighboring microfacets.
//
// We compute G for both the light direction (shadowing) and view direction
// (masking), then combine them: G = G_light × G_view.
//
// Formula: G_SchlickGGX(N·V, k) = (N·V) / ((N·V) × (1 - k) + k)
//   where k = (roughness + 1)² / 8  (for direct lighting)
fn geometry_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return n_dot_v / (n_dot_v * (1.0 - k) + k);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let ggx_v = geometry_schlick_ggx(n_dot_v, roughness);
    let ggx_l = geometry_schlick_ggx(n_dot_l, roughness);
    return ggx_v * ggx_l;
}

// Compute BRDF contribution from a single light source.
fn compute_light(
    light_dir: vec3<f32>,       // normalized direction FROM surface TO light
    light_radiance: vec3<f32>,  // light color × intensity × attenuation
    normal: vec3<f32>,
    view_dir: vec3<f32>,
    base_color: vec3<f32>,
    metallic: f32,
    roughness: f32,
    f0: vec3<f32>,
) -> vec3<f32> {
    let half_vec = normalize(view_dir + light_dir);

    let n_dot_l = max(dot(normal, light_dir), 0.0);
    let n_dot_v = max(dot(normal, view_dir), 0.001); // avoid div by zero
    let n_dot_h = max(dot(normal, half_vec), 0.0);
    let h_dot_v = max(dot(half_vec, view_dir), 0.0);

    // Cook-Torrance specular BRDF
    let d = distribution_ggx(n_dot_h, roughness);
    let f = fresnel_schlick(h_dot_v, f0);
    let g = geometry_smith(n_dot_v, n_dot_l, roughness);
    let specular = (d * f * g) / (4.0 * n_dot_v * n_dot_l + 0.0001);

    // Energy conservation: what isn't reflected is refracted (diffuse).
    // Metals have no diffuse component — they absorb refracted light.
    let k_s = f;
    let k_d = (vec3<f32>(1.0) - k_s) * (1.0 - metallic);

    // Lambertian diffuse
    let diffuse = k_d * base_color / PI;

    return (diffuse + specular) * light_radiance * n_dot_l;
}

// ── Fragment Shader ─────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample base color texture and multiply by material color
    let tex_color = textureSample(base_color_texture, base_color_sampler, in.uv);
    let base_color = tex_color.rgb * material.base_color.rgb;

    let metallic = material.metallic;
    let roughness = max(material.roughness, 0.04); // clamp to avoid singularity

    let normal = normalize(in.world_normal);
    let view_dir = normalize(camera.camera_pos - in.world_pos);

    // F0: reflectance at normal incidence
    // Dielectrics: 0.04 (4%). Metals: base_color.
    let f0 = mix(vec3<f32>(0.04), base_color, metallic);

    // Accumulate lighting
    var lo = vec3<f32>(0.0);

    // ── Directional light ───────────────────────────────────────────────
    if lights.dir_intensity > 0.0 {
        let light_dir = normalize(-lights.dir_direction);
        let radiance = lights.dir_color * lights.dir_intensity;
        lo += compute_light(light_dir, radiance, normal, view_dir, base_color, metallic, roughness, f0);
    }

    // ── Point lights ────────────────────────────────────────────────────
    for (var i = 0u; i < lights.point_light_count; i++) {
        let pl = lights.point_lights[i];
        let to_light = pl.position - in.world_pos;
        let distance = length(to_light);

        // Skip lights beyond their radius
        if distance > pl.radius {
            continue;
        }

        let light_dir = to_light / distance;

        // Inverse-square attenuation with smooth falloff at radius boundary
        //   attenuation = saturate(1 - (d/r)⁴) × 1/d²
        // The (d/r)⁴ term smoothly fades to zero at the radius edge,
        // preventing a hard cutoff.
        let d_over_r = distance / pl.radius;
        let d_over_r2 = d_over_r * d_over_r;
        let falloff = clamp(1.0 - d_over_r2 * d_over_r2, 0.0, 1.0);
        let attenuation = falloff / (distance * distance + 0.0001);

        let radiance = pl.color * pl.intensity * attenuation;
        lo += compute_light(light_dir, radiance, normal, view_dir, base_color, metallic, roughness, f0);
    }

    // ── Ambient ─────────────────────────────────────────────────────────
    // A simple constant term to prevent pure-black shadows. In a production
    // renderer, this would be replaced by image-based lighting (IBL) or
    // screen-space ambient occlusion (SSAO).
    let ambient = lights.ambient_color * lights.ambient_intensity * base_color;

    // ── Final color ─────────────────────────────────────────────────────
    var color = ambient + lo + material.emissive;

    // Simple Reinhard tone mapping: maps HDR [0, ∞) to LDR [0, 1)
    // Without this, bright highlights would clip to white.
    color = color / (color + vec3<f32>(1.0));
    return vec4<f32>(color, 1.0);
}
