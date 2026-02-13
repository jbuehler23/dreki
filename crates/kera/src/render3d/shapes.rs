//! # Shapes — Built-In Mesh Generators
//!
//! Primitive meshes (cube, plane, sphere) are the building blocks of any 3D
//! scene. This module generates vertex and index data for each shape, ready to
//! upload to the GPU via [`MeshStore`](super::mesh::MeshStore).
//!
//! ## Why Generate on CPU?
//!
//! These are simple enough to compute once at startup. The alternative —
//! loading `.obj` or `.glb` files for basic shapes — adds file I/O and
//! parsing overhead for geometry that fits in a few hundred bytes. For complex
//! models (characters, environments), use the glTF loader instead.
//!
//! ## Winding Order and Normals
//!
//! All triangles use counter-clockwise (CCW) winding when viewed from the
//! front face. This matches wgpu's default `FrontFace::Ccw` setting and
//! ensures backface culling works correctly. Each vertex has a surface normal
//! pointing outward — the direction the face is "looking."
//!
//! For a cube, each face needs its own set of 4 vertices (24 total) even
//! though a cube has only 8 unique positions. This is because vertices on
//! shared edges need different normals for each face — a corner vertex on
//! the top face has normal (0,1,0) while the same corner on the front face
//! has normal (0,0,1). Sharing vertices would produce incorrect lighting at
//! edges.
//!
//! ## UV Mapping
//!
//! Texture coordinates (UVs) map the [0,1]² unit square onto each face.
//! For a cube, each face gets the full [0,1]² range — a texture applied to
//! a cube repeats on each face. The sphere uses equirectangular mapping
//! (longitude → U, latitude → V), which produces distortion at the poles
//! but is the simplest approach.
//!
//! ## Comparison
//!
//! - **Bevy**: `bevy_render::mesh::shape` provides similar primitives plus
//!   capsule, torus, and icosphere. Uses `Mesh` with attribute arrays.
//! - **three.js**: `BoxGeometry`, `PlaneGeometry`, `SphereGeometry` with
//!   configurable subdivisions. Very similar API.
//! - **Unity**: Built-in primitives accessed via `GameObject.CreatePrimitive()`.

use super::vertex::MeshVertex;

/// Generate a unit cube centered at the origin (side length 1.0).
///
/// Returns 24 vertices (4 per face for correct normals) and 36 indices.
pub(crate) fn cube() -> (Vec<MeshVertex>, Vec<u32>) {
    // Each face has 4 vertices with a shared normal.
    // Vertices are ordered for CCW winding when viewed from outside.
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    // (normal, tangent_u, tangent_v) for each face
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        // +X (right)
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        // -X (left)
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        // +Y (top)
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        // -Y (bottom)
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        // +Z (front)
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        // -Z (back)
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];

    for (normal, u_dir, v_dir) in &faces {
        let base = vertices.len() as u32;
        let h = 0.5_f32;

        // Center of this face
        let center = [normal[0] * h, normal[1] * h, normal[2] * h];

        // Four corners: center ± u_dir*h ± v_dir*h
        let corners = [
            [-1.0, -1.0], // bottom-left  (uv 0,1)
            [1.0, -1.0],  // bottom-right (uv 1,1)
            [1.0, 1.0],   // top-right    (uv 1,0)
            [-1.0, 1.0],  // top-left     (uv 0,0)
        ];

        let uvs = [
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
        ];

        for (i, corner) in corners.iter().enumerate() {
            let pos = [
                center[0] + u_dir[0] * corner[0] * h + v_dir[0] * corner[1] * h,
                center[1] + u_dir[1] * corner[0] * h + v_dir[1] * corner[1] * h,
                center[2] + u_dir[2] * corner[0] * h + v_dir[2] * corner[1] * h,
            ];
            vertices.push(MeshVertex {
                position: pos,
                normal: *normal,
                uv: uvs[i],
            });
        }

        // Two triangles per face (CCW)
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    (vertices, indices)
}

/// Generate a plane on the XZ plane (normal +Y), centered at origin.
///
/// Returns 4 vertices and 6 indices. Side length 1.0.
pub(crate) fn plane() -> (Vec<MeshVertex>, Vec<u32>) {
    let h = 0.5_f32;
    let vertices = vec![
        MeshVertex { position: [-h, 0.0, h], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0] },
        MeshVertex { position: [h, 0.0, h], normal: [0.0, 1.0, 0.0], uv: [1.0, 0.0] },
        MeshVertex { position: [h, 0.0, -h], normal: [0.0, 1.0, 0.0], uv: [1.0, 1.0] },
        MeshVertex { position: [-h, 0.0, -h], normal: [0.0, 1.0, 0.0], uv: [0.0, 1.0] },
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    (vertices, indices)
}

/// Generate a UV sphere centered at origin with radius 0.5.
///
/// `segments` is the number of horizontal divisions (longitude), `rings` is
/// the number of vertical divisions (latitude). Default: 32 segments, 16 rings.
///
/// UV mapping: U wraps around longitude [0, 1], V goes from north pole (0)
/// to south pole (1). This produces distortion near the poles but is simple
/// and matches most 3D software conventions.
pub(crate) fn sphere(segments: u32, rings: u32) -> (Vec<MeshVertex>, Vec<u32>) {
    let radius = 0.5_f32;
    let mut vertices = Vec::with_capacity(((rings + 1) * (segments + 1)) as usize);
    let mut indices = Vec::with_capacity((rings * segments * 6) as usize);

    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        let phi = v * std::f32::consts::PI; // 0 at top, π at bottom

        for seg in 0..=segments {
            let u = seg as f32 / segments as f32;
            let theta = u * 2.0 * std::f32::consts::PI; // 0..2π around

            let x = phi.sin() * theta.cos();
            let y = phi.cos();
            let z = phi.sin() * theta.sin();

            vertices.push(MeshVertex {
                position: [x * radius, y * radius, z * radius],
                normal: [x, y, z],
                uv: [u, v],
            });
        }
    }

    // Generate triangle indices
    for ring in 0..rings {
        for seg in 0..segments {
            let current = ring * (segments + 1) + seg;
            let next = current + segments + 1;

            // Two triangles per quad (CCW)
            indices.extend_from_slice(&[current, next, current + 1]);
            indices.extend_from_slice(&[current + 1, next, next + 1]);
        }
    }

    (vertices, indices)
}

/// Generate a cylinder centered at the origin with configurable radius and half-height.
///
/// The cylinder is oriented along the Y axis. `radius` is the XZ radius,
/// `half_height` is the distance from center to each cap (total height = 2 × half_height).
/// `segments` is the number of divisions around the circumference.
///
/// Returns vertices and indices for:
/// - Side quads with smooth normals (pointing radially outward)
/// - Top cap (normal +Y) with a center fan
/// - Bottom cap (normal −Y) with a center fan
pub(crate) fn cylinder(radius: f32, half_height: f32, segments: u32) -> (Vec<MeshVertex>, Vec<u32>) {
    let seg = segments.max(3);
    // Side: (seg+1)*2, Top cap: seg+1 (center + rim), Bottom cap: seg+1
    let vert_count = ((seg + 1) * 2 + (seg + 1) + (seg + 1)) as usize;
    let idx_count = (seg * 6 + seg * 3 + seg * 3) as usize;
    let mut vertices = Vec::with_capacity(vert_count);
    let mut indices = Vec::with_capacity(idx_count);

    let pi2 = std::f32::consts::PI * 2.0;

    // ── Side vertices ──
    // Two rings (top and bottom) with seg+1 vertices each for UV seam.
    for i in 0..=seg {
        let u = i as f32 / seg as f32;
        let theta = u * pi2;
        let cos = theta.cos();
        let sin = theta.sin();

        // Top ring
        vertices.push(MeshVertex {
            position: [cos * radius, half_height, sin * radius],
            normal: [cos, 0.0, sin],
            uv: [u, 0.0],
        });
        // Bottom ring
        vertices.push(MeshVertex {
            position: [cos * radius, -half_height, sin * radius],
            normal: [cos, 0.0, sin],
            uv: [u, 1.0],
        });
    }

    // Side indices (quads → two triangles each)
    for i in 0..seg {
        let top0 = i * 2;
        let bot0 = top0 + 1;
        let top1 = top0 + 2;
        let bot1 = top0 + 3;
        // CCW when viewed from outside
        indices.extend_from_slice(&[top0, bot0, bot1, top0, bot1, top1]);
    }

    // ── Top cap ──
    let top_center = vertices.len() as u32;
    vertices.push(MeshVertex {
        position: [0.0, half_height, 0.0],
        normal: [0.0, 1.0, 0.0],
        uv: [0.5, 0.5],
    });
    for i in 0..seg {
        let theta = i as f32 / seg as f32 * pi2;
        let cos = theta.cos();
        let sin = theta.sin();
        vertices.push(MeshVertex {
            position: [cos * radius, half_height, sin * radius],
            normal: [0.0, 1.0, 0.0],
            uv: [0.5 + cos * 0.5, 0.5 + sin * 0.5],
        });
    }
    for i in 0..seg {
        let curr = top_center + 1 + i;
        let next = top_center + 1 + (i + 1) % seg;
        indices.extend_from_slice(&[top_center, curr, next]);
    }

    // ── Bottom cap ──
    let bot_center = vertices.len() as u32;
    vertices.push(MeshVertex {
        position: [0.0, -half_height, 0.0],
        normal: [0.0, -1.0, 0.0],
        uv: [0.5, 0.5],
    });
    for i in 0..seg {
        let theta = i as f32 / seg as f32 * pi2;
        let cos = theta.cos();
        let sin = theta.sin();
        vertices.push(MeshVertex {
            position: [cos * radius, -half_height, sin * radius],
            normal: [0.0, -1.0, 0.0],
            uv: [0.5 + cos * 0.5, 0.5 + sin * 0.5],
        });
    }
    for i in 0..seg {
        let curr = bot_center + 1 + i;
        let next = bot_center + 1 + (i + 1) % seg;
        // CCW from below means reversed winding vs top
        indices.extend_from_slice(&[bot_center, next, curr]);
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_has_correct_counts() {
        let (verts, idxs) = cube();
        assert_eq!(verts.len(), 24, "cube should have 24 vertices (4 per face)");
        assert_eq!(idxs.len(), 36, "cube should have 36 indices (6 per face)");
    }

    #[test]
    fn cube_indices_in_range() {
        let (verts, idxs) = cube();
        for &idx in &idxs {
            assert!((idx as usize) < verts.len(), "index {idx} out of range");
        }
    }

    #[test]
    fn cube_normals_are_unit_length() {
        let (verts, _) = cube();
        for v in &verts {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!((len - 1.0).abs() < 1e-6, "normal should be unit length, got {len}");
        }
    }

    #[test]
    fn plane_has_correct_counts() {
        let (verts, idxs) = plane();
        assert_eq!(verts.len(), 4);
        assert_eq!(idxs.len(), 6);
    }

    #[test]
    fn plane_normals_point_up() {
        let (verts, _) = plane();
        for v in &verts {
            assert_eq!(v.normal, [0.0, 1.0, 0.0]);
        }
    }

    #[test]
    fn sphere_has_correct_counts() {
        let (verts, idxs) = sphere(32, 16);
        assert_eq!(verts.len(), (17 * 33) as usize);
        assert_eq!(idxs.len(), (16 * 32 * 6) as usize);
    }

    #[test]
    fn sphere_normals_are_unit_length() {
        let (verts, _) = sphere(8, 4);
        for v in &verts {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "normal should be unit length, got {len}");
        }
    }

    #[test]
    fn sphere_indices_in_range() {
        let (verts, idxs) = sphere(8, 4);
        for &idx in &idxs {
            assert!((idx as usize) < verts.len(), "index {idx} out of range");
        }
    }

    #[test]
    fn cylinder_indices_in_range() {
        let (verts, idxs) = cylinder(0.5, 0.5, 32);
        for &idx in &idxs {
            assert!((idx as usize) < verts.len(), "index {idx} out of range");
        }
    }

    #[test]
    fn cylinder_normals_are_unit_length() {
        let (verts, _) = cylinder(0.5, 0.5, 16);
        for v in &verts {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "normal should be unit length, got {len}");
        }
    }

    #[test]
    fn cylinder_index_count_matches() {
        let seg = 32u32;
        let (_, idxs) = cylinder(0.5, 0.5, seg);
        // side: seg*6, top cap: seg*3, bottom cap: seg*3
        let expected = (seg * 6 + seg * 3 + seg * 3) as usize;
        assert_eq!(idxs.len(), expected);
    }
}
