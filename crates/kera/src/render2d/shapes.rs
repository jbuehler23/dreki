//! # Shape2d — First-Class 2D Shape Primitives
//!
//! Draw 2D shapes (circles, rectangles, triangles, polygons) with a single
//! component instead of requiring texture hacks.
//!
//! ```ignore
//! world.spawn((
//!     Transform::from_xy(0.0, 0.0),
//!     Shape2d::circle(20.0).color(Color::RED),
//! ));
//! ```
//!
//! Shapes are CPU-tessellated each frame into the same vertex/index buffers as
//! sprites. They use the built-in 1x1 white texture (handle 0) so they batch
//! naturally with untextured sprites.

use super::Color;
use crate::math::Vec2;

/// The kind and dimensions of a 2D shape primitive.
#[derive(Debug, Clone)]
pub enum ShapeKind2d {
    Circle { radius: f32, segments: u32 },
    Rectangle { width: f32, height: f32 },
    Triangle { points: [Vec2; 3] },
    Polygon { points: Vec<Vec2> },
}

/// A 2D shape component. Pair with [`Transform`](crate::math::Transform) to render.
///
/// Shapes are tessellated into triangles each frame and drawn through the
/// same sprite pipeline using the default white texture.
#[derive(Debug, Clone)]
pub struct Shape2d {
    pub kind: ShapeKind2d,
    pub color: Color,
}

impl Shape2d {
    /// A circle with the given radius. Default 32 segments, white.
    pub fn circle(radius: f32) -> Self {
        Self {
            kind: ShapeKind2d::Circle { radius, segments: 32 },
            color: Color::WHITE,
        }
    }

    /// A rectangle with the given width and height, centered at origin.
    pub fn rectangle(width: f32, height: f32) -> Self {
        Self {
            kind: ShapeKind2d::Rectangle { width, height },
            color: Color::WHITE,
        }
    }

    /// A triangle defined by three points in local space.
    pub fn triangle(a: Vec2, b: Vec2, c: Vec2) -> Self {
        Self {
            kind: ShapeKind2d::Triangle { points: [a, b, c] },
            color: Color::WHITE,
        }
    }

    /// A convex polygon defined by its vertices in local space.
    /// Tessellated as a fan from the centroid (convex shapes only).
    pub fn polygon(points: Vec<Vec2>) -> Self {
        Self {
            kind: ShapeKind2d::Polygon { points },
            color: Color::WHITE,
        }
    }

    /// Set the shape color.
    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }

    /// Tessellate this shape into local-space positions and triangle indices.
    pub(crate) fn tessellate(&self) -> (Vec<[f32; 2]>, Vec<u32>) {
        match &self.kind {
            ShapeKind2d::Circle { radius, segments } => tessellate_circle(*radius, *segments),
            ShapeKind2d::Rectangle { width, height } => tessellate_rectangle(*width, *height),
            ShapeKind2d::Triangle { points } => tessellate_triangle(points),
            ShapeKind2d::Polygon { points } => tessellate_polygon(points),
        }
    }
}

/// Circle: center vertex + rim vertices, fan triangulation.
fn tessellate_circle(radius: f32, segments: u32) -> (Vec<[f32; 2]>, Vec<u32>) {
    let seg = segments.max(3);
    let mut verts = Vec::with_capacity(seg as usize + 1);
    let mut idxs = Vec::with_capacity(seg as usize * 3);

    // Center vertex
    verts.push([0.0, 0.0]);

    let pi2 = std::f32::consts::PI * 2.0;
    for i in 0..seg {
        let theta = i as f32 / seg as f32 * pi2;
        verts.push([theta.cos() * radius, theta.sin() * radius]);
    }

    for i in 0..seg {
        let curr = 1 + i;
        let next = 1 + (i + 1) % seg;
        idxs.extend_from_slice(&[0, curr, next]);
    }

    (verts, idxs)
}

/// Rectangle: 4 vertices, 2 triangles.
fn tessellate_rectangle(width: f32, height: f32) -> (Vec<[f32; 2]>, Vec<u32>) {
    let hw = width * 0.5;
    let hh = height * 0.5;
    let verts = vec![
        [-hw, -hh], // 0: bottom-left
        [ hw, -hh], // 1: bottom-right
        [ hw,  hh], // 2: top-right
        [-hw,  hh], // 3: top-left
    ];
    let idxs = vec![0, 1, 2, 0, 2, 3];
    (verts, idxs)
}

/// Triangle: 3 vertices, 1 triangle.
fn tessellate_triangle(points: &[Vec2; 3]) -> (Vec<[f32; 2]>, Vec<u32>) {
    let verts = vec![
        [points[0].x, points[0].y],
        [points[1].x, points[1].y],
        [points[2].x, points[2].y],
    ];
    let idxs = vec![0, 1, 2];
    (verts, idxs)
}

/// Convex polygon: fan from centroid.
fn tessellate_polygon(points: &[Vec2]) -> (Vec<[f32; 2]>, Vec<u32>) {
    if points.len() < 3 {
        return (Vec::new(), Vec::new());
    }

    let n = points.len();
    let mut verts = Vec::with_capacity(n + 1);
    let mut idxs = Vec::with_capacity(n * 3);

    // Centroid
    let cx = points.iter().map(|p| p.x).sum::<f32>() / n as f32;
    let cy = points.iter().map(|p| p.y).sum::<f32>() / n as f32;
    verts.push([cx, cy]);

    for p in points {
        verts.push([p.x, p.y]);
    }

    for i in 0..n as u32 {
        let curr = 1 + i;
        let next = 1 + (i + 1) % n as u32;
        idxs.extend_from_slice(&[0, curr, next]);
    }

    (verts, idxs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_indices_in_range() {
        let shape = Shape2d::circle(10.0);
        let (verts, idxs) = shape.tessellate();
        for &idx in &idxs {
            assert!((idx as usize) < verts.len(), "index {idx} out of range");
        }
        // 32 segments → 33 verts (center + 32 rim), 96 indices
        assert_eq!(verts.len(), 33);
        assert_eq!(idxs.len(), 96);
    }

    #[test]
    fn rectangle_basic() {
        let shape = Shape2d::rectangle(10.0, 6.0);
        let (verts, idxs) = shape.tessellate();
        assert_eq!(verts.len(), 4);
        assert_eq!(idxs.len(), 6);
    }

    #[test]
    fn triangle_basic() {
        let shape = Shape2d::triangle(
            Vec2::new(0.0, 1.0),
            Vec2::new(-1.0, -1.0),
            Vec2::new(1.0, -1.0),
        );
        let (verts, idxs) = shape.tessellate();
        assert_eq!(verts.len(), 3);
        assert_eq!(idxs.len(), 3);
    }

    #[test]
    fn polygon_indices_in_range() {
        let pts = vec![
            Vec2::new(0.0, 2.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(0.0, -2.0),
            Vec2::new(-2.0, 0.0),
        ];
        let shape = Shape2d::polygon(pts);
        let (verts, idxs) = shape.tessellate();
        assert_eq!(verts.len(), 5); // centroid + 4
        assert_eq!(idxs.len(), 12); // 4 triangles × 3
        for &idx in &idxs {
            assert!((idx as usize) < verts.len());
        }
    }
}
