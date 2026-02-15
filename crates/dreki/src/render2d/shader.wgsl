// ============================================================================
// Shader — Programs That Run on the GPU
//
// A shader is a small program executed by the GPU in parallel. This file
// contains two shaders written in WGSL (WebGPU Shading Language):
//
//   Vertex shader (vs_main)
//     Runs once per vertex. Its job: transform a vertex position from
//     world space into "clip space" — the normalized [-1, 1] coordinate
//     system the GPU rasterizer expects. It also passes through UV
//     coordinates and tint color for the fragment shader to use.
//
//   Fragment shader (fs_main)
//     Runs once per pixel (fragment) covered by a triangle. Its job:
//     determine the final color. It samples the texture at the
//     interpolated UV coordinate and multiplies by the tint color.
//
// Data flow through the pipeline:
//
//   CPU vertex buffer          Vertex Shader              Rasterizer
//   ┌─────────────┐      ┌─────────────────────┐     ┌──────────────┐
//   │ position     │─────►│ clip_pos = VP × pos │────►│ interpolate  │
//   │ uv           │─────►│ uv (pass through)   │────►│ uv, color    │
//   │ color        │─────►│ color (pass through) │────►│ per-pixel    │
//   └─────────────┘      └─────────────────────┘     └──────┬───────┘
//                                                            │
//                          Fragment Shader                    │
//                    ┌────────────────────────────┐          │
//                    │ sample texture at uv       │◄─────────┘
//                    │ multiply by tint color     │
//                    │ → final pixel color        │
//                    └────────────────────────────┘
//
// Bind Groups
//
// The GPU needs access to data beyond vertex attributes: the camera matrix
// and the sprite texture. These are provided via "bind groups" — named
// slots that connect GPU resources to shader variables.
//
//   Group 0 (set once per frame): camera view-projection matrix
//     A 4x4 float uniform. The vertex shader reads it to transform
//     positions. It never changes within a frame.
//
//   Group 1 (set once per texture batch): sprite texture + sampler
//     The texture is the image data. The sampler controls how texels
//     are filtered when the sprite is scaled or rotated (we use Nearest
//     for pixel-art crispness). This group changes each time we switch
//     to a different texture during rendering.
//
// The split between groups is intentional: group 0 is bound once and left
// alone, while group 1 is rebound for each texture batch. wgpu tracks bind
// group state, so only the changed group triggers a state update.
// ============================================================================

// Group 0: camera uniform (set once per frame)
@group(0) @binding(0)
var<uniform> camera: mat4x4<f32>;

// Group 1: per-batch texture + sampler
@group(1) @binding(0)
var sprite_texture: texture_2d<f32>;
@group(1) @binding(1)
var sprite_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera * vec4<f32>(in.position, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(sprite_texture, sprite_sampler, in.uv);
    return tex_color * in.color;
}
