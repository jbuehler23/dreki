//! # Entity Hierarchies — Parent/Child Relationships
//!
//! Provides [`Parent`], [`Children`], and [`GlobalTransform`] components for
//! expressing entity hierarchies and propagating transforms from parent to child.
//!
//! ## Usage
//!
//! ```ignore
//! // Spawn a parent with children.
//! let parent = world.spawn((Transform::from_xy(100.0, 50.0),));
//! let child = world.spawn_child(parent, (Transform::from_xy(10.0, 0.0),));
//!
//! // After running propagate_transforms, child's GlobalTransform
//! // reflects the combined parent + child transform.
//! propagate_transforms(&mut world);
//! ```

use std::collections::VecDeque;


use crate::ecs::world::World;
use crate::math::{Mat4, Transform};

/// Marks an entity as a child of another entity.
#[derive(Debug, Clone, Copy)]
pub struct Parent(pub crate::ecs::Entity);

/// Stores the list of child entities for a parent.
#[derive(Debug, Clone)]
pub struct Children(pub Vec<crate::ecs::Entity>);

/// The world-space transform computed by [`propagate_transforms`].
///
/// For root entities (no [`Parent`]), this equals the local [`Transform`].
/// For children, this is `parent_global * child_local`.
#[derive(Debug, Clone, Copy, Default)]
pub struct GlobalTransform {
    pub matrix: Mat4,
}

/// Propagate local transforms down the entity hierarchy.
///
/// - Roots (entities with `Transform` but no `Parent`) get `GlobalTransform = Transform.matrix()`.
/// - Children get `GlobalTransform = parent_global * child_local.matrix()`.
/// - Traversal is BFS to ensure parents are computed before children.
pub fn propagate_transforms(world: &mut World) {
    // Collect root entities: have Transform but no Parent.
    let mut roots = Vec::new();
    world.query::<(&Transform,)>(|entity, (transform,)| {
        roots.push((entity, transform.matrix()));
    });

    // Filter to only roots (no Parent component).
    let roots: Vec<_> = roots
        .into_iter()
        .filter(|(entity, _)| world.get::<Parent>(*entity).is_none())
        .collect();

    // Set GlobalTransform on roots and BFS through children.
    let mut queue: VecDeque<(crate::ecs::Entity, Mat4)> = VecDeque::new();

    for (entity, matrix) in roots {
        world.insert(entity, GlobalTransform { matrix });

        // Enqueue children.
        if let Some(children) = world.get::<Children>(entity) {
            let child_list: Vec<_> = children.0.clone();
            for child in child_list {
                queue.push_back((child, matrix));
            }
        }
    }

    // BFS propagation.
    while let Some((entity, parent_matrix)) = queue.pop_front() {
        let local_matrix = world
            .get::<Transform>(entity)
            .map(|t| t.matrix())
            .unwrap_or(Mat4::IDENTITY);
        let global_matrix = parent_matrix * local_matrix;
        world.insert(entity, GlobalTransform { matrix: global_matrix });

        if let Some(children) = world.get::<Children>(entity) {
            let child_list: Vec<_> = children.0.clone();
            for child in child_list {
                queue.push_back((child, global_matrix));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Transform, Vec3};

    #[test]
    fn root_gets_global_transform() {
        let mut world = World::new();
        let root = world.spawn((Transform::from_xyz(10.0, 20.0, 0.0),));

        propagate_transforms(&mut world);

        let gt = world.get::<GlobalTransform>(root).unwrap();
        let expected = Transform::from_xyz(10.0, 20.0, 0.0).matrix();
        assert_eq!(gt.matrix, expected);
    }

    #[test]
    fn child_inherits_parent_transform() {
        let mut world = World::new();
        let parent = world.spawn((Transform::from_xyz(100.0, 0.0, 0.0),));
        let child = world.spawn_child(parent, (Transform::from_xyz(10.0, 0.0, 0.0),));

        propagate_transforms(&mut world);

        let gt = world.get::<GlobalTransform>(child).unwrap();
        // Child at local (10,0,0) under parent at (100,0,0) → global (110,0,0)
        let col3 = gt.matrix.col(3);
        assert!((col3.x - 110.0).abs() < 0.001);
        assert!((col3.y - 0.0).abs() < 0.001);
    }

    #[test]
    fn parent_moves_child_follows() {
        let mut world = World::new();
        let parent = world.spawn((Transform::from_xyz(0.0, 0.0, 0.0),));
        let child = world.spawn_child(parent, (Transform::from_xyz(5.0, 0.0, 0.0),));

        propagate_transforms(&mut world);

        // Move parent.
        world.get_mut::<Transform>(parent).unwrap().translation = Vec3::new(50.0, 0.0, 0.0);
        propagate_transforms(&mut world);

        let gt = world.get::<GlobalTransform>(child).unwrap();
        let col3 = gt.matrix.col(3);
        assert!((col3.x - 55.0).abs() < 0.001);
    }

    #[test]
    fn despawn_recursive_removes_children() {
        let mut world = World::new();
        let parent = world.spawn((Transform::default(),));
        let child1 = world.spawn_child(parent, (Transform::default(),));
        let grandchild = world.spawn_child(child1, (Transform::default(),));
        let _child2 = world.spawn_child(parent, (Transform::default(),));

        assert_eq!(world.entity_count(), 4);

        world.despawn_recursive(parent);

        assert_eq!(world.entity_count(), 0);
        assert!(!world.is_alive(parent));
        assert!(!world.is_alive(child1));
        assert!(!world.is_alive(grandchild));
    }

    #[test]
    fn despawn_recursive_cleans_up_parent_children() {
        let mut world = World::new();
        let parent = world.spawn((Transform::default(),));
        let child1 = world.spawn_child(parent, (Transform::default(),));
        let _child2 = world.spawn_child(parent, (Transform::default(),));

        // Despawn child1 (not the parent).
        world.despawn_recursive(child1);

        // Parent should still exist, with only child2 remaining.
        assert!(world.is_alive(parent));
        let children = world.get::<Children>(parent).unwrap();
        assert_eq!(children.0.len(), 1);
    }

    #[test]
    fn despawn_all_clears_world() {
        let mut world = World::new();
        world.spawn((Transform::default(),));
        world.spawn((Transform::default(),));
        world.spawn((Transform::default(),));

        assert_eq!(world.entity_count(), 3);
        world.despawn_all();
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn deep_hierarchy_propagation() {
        let mut world = World::new();
        let a = world.spawn((Transform::from_xyz(1.0, 0.0, 0.0),));
        let b = world.spawn_child(a, (Transform::from_xyz(2.0, 0.0, 0.0),));
        let c = world.spawn_child(b, (Transform::from_xyz(3.0, 0.0, 0.0),));

        propagate_transforms(&mut world);

        let gt_c = world.get::<GlobalTransform>(c).unwrap();
        let col3 = gt_c.matrix.col(3);
        assert!((col3.x - 6.0).abs() < 0.001); // 1 + 2 + 3
    }
}
