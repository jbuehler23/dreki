// Debug wireframe shader for 2D collider visualization.
// Vertices are pre-transformed to world space on the CPU.

struct Camera {
    view_proj: mat4x4<f32>,
}

struct WireframeParams {
    color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<uniform> params: WireframeParams;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

@vertex
fn vs_main(@location(0) position: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4(position, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return params.color;
}
