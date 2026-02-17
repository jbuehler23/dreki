//! 3D physics integration via Rapier.
//!
//! Provides rigid-body and collider components that automatically synchronize
//! with an internal Rapier simulation. Add [`PhysicsWorld3d`] as a resource,
//! attach [`RigidBody3d`] and [`Collider3d`] to your entities, and run
//! [`physics_step_3d`] each frame.

use std::collections::HashMap;

use rapier3d::prelude::*;

use crate::ecs::{Entity, World};
use crate::math::{Quat, Transform};

// ── Conversion helpers ──────────────────────────────────────────────────

fn body_type_to_rapier(bt: RigidBodyType3d) -> RigidBodyType {
    match bt {
        RigidBodyType3d::Dynamic => RigidBodyType::Dynamic,
        RigidBodyType3d::Fixed => RigidBodyType::Fixed,
        RigidBodyType3d::KinematicPositionBased => RigidBodyType::KinematicPositionBased,
        RigidBodyType3d::KinematicVelocityBased => RigidBodyType::KinematicVelocityBased,
    }
}

fn shape_to_collider_builder(shape: &ColliderShape3d) -> ColliderBuilder {
    match *shape {
        ColliderShape3d::Ball { radius } => ColliderBuilder::ball(radius),
        ColliderShape3d::Cuboid { hx, hy, hz } => ColliderBuilder::cuboid(hx, hy, hz),
        ColliderShape3d::CapsuleY {
            half_height,
            radius,
        } => ColliderBuilder::capsule_y(half_height, radius),
        ColliderShape3d::CapsuleX {
            half_height,
            radius,
        } => ColliderBuilder::capsule_x(half_height, radius),
        ColliderShape3d::CapsuleZ {
            half_height,
            radius,
        } => ColliderBuilder::capsule_z(half_height, radius),
    }
}

/// Convert a glam Quat to a scaled-axis-angle Vec3 (for RigidBodyBuilder::rotation).
fn quat_to_scaled_axis(q: Quat) -> Vec3 {
    let (axis, angle) = q.to_axis_angle();
    axis * angle
}

// ── Components ──────────────────────────────────────────────────────────

/// Rigid-body type for 3D physics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigidBodyType3d {
    Dynamic,
    Fixed,
    KinematicPositionBased,
    KinematicVelocityBased,
}

/// A 3D rigid-body component.
///
/// Attach to an entity alongside a [`Transform`] to give it physical behaviour.
#[derive(Debug, Clone)]
pub struct RigidBody3d {
    pub body_type: RigidBodyType3d,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub gravity_scale: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub ccd_enabled: bool,
    pub(crate) handle: Option<RigidBodyHandle>,
}

impl RigidBody3d {
    /// A dynamic body affected by gravity and forces.
    pub fn dynamic() -> Self {
        Self {
            body_type: RigidBodyType3d::Dynamic,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            gravity_scale: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            ccd_enabled: false,
            handle: None,
        }
    }

    /// A fixed (static) body that never moves.
    pub fn fixed() -> Self {
        Self {
            body_type: RigidBodyType3d::Fixed,
            ..Self::dynamic()
        }
    }

    /// A kinematic body driven by position updates.
    pub fn kinematic_position() -> Self {
        Self {
            body_type: RigidBodyType3d::KinematicPositionBased,
            ..Self::dynamic()
        }
    }

    /// A kinematic body driven by velocity.
    pub fn kinematic_velocity() -> Self {
        Self {
            body_type: RigidBodyType3d::KinematicVelocityBased,
            ..Self::dynamic()
        }
    }

    pub fn with_linear_velocity(mut self, v: Vec3) -> Self {
        self.linear_velocity = v;
        self
    }

    pub fn with_angular_velocity(mut self, v: Vec3) -> Self {
        self.angular_velocity = v;
        self
    }

    pub fn with_gravity_scale(mut self, s: f32) -> Self {
        self.gravity_scale = s;
        self
    }

    pub fn with_linear_damping(mut self, d: f32) -> Self {
        self.linear_damping = d;
        self
    }

    pub fn with_angular_damping(mut self, d: f32) -> Self {
        self.angular_damping = d;
        self
    }

    pub fn with_ccd(mut self, enabled: bool) -> Self {
        self.ccd_enabled = enabled;
        self
    }
}

/// Collider shape for 3D physics.
#[derive(Debug, Clone, Copy)]
pub enum ColliderShape3d {
    Ball { radius: f32 },
    Cuboid { hx: f32, hy: f32, hz: f32 },
    CapsuleY { half_height: f32, radius: f32 },
    CapsuleX { half_height: f32, radius: f32 },
    CapsuleZ { half_height: f32, radius: f32 },
}

/// A 3D collider component.
///
/// Attach alongside a [`RigidBody3d`]. The Rapier handle is managed internally.
#[derive(Debug, Clone)]
pub struct Collider3d {
    pub shape: ColliderShape3d,
    pub restitution: f32,
    pub friction: f32,
    pub density: f32,
    pub sensor: bool,
    pub(crate) handle: Option<ColliderHandle>,
}

impl Collider3d {
    /// A spherical collider.
    pub fn ball(radius: f32) -> Self {
        Self {
            shape: ColliderShape3d::Ball { radius },
            restitution: 0.0,
            friction: 0.5,
            density: 1.0,
            sensor: false,
            handle: None,
        }
    }

    /// A box collider (half-extents).
    pub fn cuboid(hx: f32, hy: f32, hz: f32) -> Self {
        Self {
            shape: ColliderShape3d::Cuboid { hx, hy, hz },
            ..Self::ball(0.0)
        }
    }

    /// A vertical capsule collider.
    pub fn capsule_y(half_height: f32, radius: f32) -> Self {
        Self {
            shape: ColliderShape3d::CapsuleY {
                half_height,
                radius,
            },
            ..Self::ball(0.0)
        }
    }

    /// A horizontal capsule collider (X axis).
    pub fn capsule_x(half_height: f32, radius: f32) -> Self {
        Self {
            shape: ColliderShape3d::CapsuleX {
                half_height,
                radius,
            },
            ..Self::ball(0.0)
        }
    }

    /// A horizontal capsule collider (Z axis).
    pub fn capsule_z(half_height: f32, radius: f32) -> Self {
        Self {
            shape: ColliderShape3d::CapsuleZ {
                half_height,
                radius,
            },
            ..Self::ball(0.0)
        }
    }

    pub fn with_restitution(mut self, r: f32) -> Self {
        self.restitution = r;
        self
    }

    pub fn with_friction(mut self, f: f32) -> Self {
        self.friction = f;
        self
    }

    pub fn with_density(mut self, d: f32) -> Self {
        self.density = d;
        self
    }

    pub fn with_sensor(mut self, s: bool) -> Self {
        self.sensor = s;
        self
    }
}

// ── Resource ────────────────────────────────────────────────────────────

/// The 3D physics world. Insert as a resource and run [`physics_step_3d`] each frame.
pub struct PhysicsWorld3d {
    gravity: Vec3,
    pipeline: PhysicsPipeline,
    params: IntegrationParameters,
    islands: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    body_to_entity: HashMap<RigidBodyHandle, Entity>,
    entity_to_body: HashMap<u32, RigidBodyHandle>,
    accumulator: f32,
}

impl std::fmt::Debug for PhysicsWorld3d {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysicsWorld3d")
            .field("gravity", &self.gravity)
            .field("bodies", &self.bodies.len())
            .field("colliders", &self.colliders.len())
            .finish()
    }
}

impl PhysicsWorld3d {
    /// Create a new physics world with default gravity (0, -9.81, 0).
    pub fn new() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            pipeline: PhysicsPipeline::new(),
            params: IntegrationParameters::default(),
            islands: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            body_to_entity: HashMap::new(),
            entity_to_body: HashMap::new(),
            accumulator: 0.0,
        }
    }

    /// Set gravity (builder pattern).
    pub fn with_gravity(mut self, g: Vec3) -> Self {
        self.gravity = g;
        self
    }
}

impl Default for PhysicsWorld3d {
    fn default() -> Self {
        Self::new()
    }
}

// ── Plugin ──────────────────────────────────────────────────────────────

/// Plugin that registers the 3D physics resource and update system.
///
/// # Example
///
/// ```ignore
/// Game::new("My Game")
///     .plugin(Physics3d)
///     .setup(setup)
///     .run();
/// ```
pub struct Physics3d;

impl crate::game::Plugin for Physics3d {
    fn build(&self, game: &mut crate::game::Game) {
        game.insert_resource(PhysicsWorld3d::new());
        game.add_update_system(|ctx| physics_step_3d(&mut ctx.world));
    }
}

// ── System ──────────────────────────────────────────────────────────────

/// Advance the 3D physics simulation by one frame.
///
/// Uses the extract/reinsert pattern to borrow the physics world and the ECS
/// world simultaneously. Physics runs with a fixed timestep (default 1/60s)
/// using an accumulator to decouple simulation from frame rate.
pub(crate) fn physics_step_3d(world: &mut World) {
    let frame_dt = world.resource::<crate::time::Time>().delta_secs();
    if frame_dt <= 0.0 {
        return;
    }

    let Some(mut pw) = world.resource_remove::<PhysicsWorld3d>() else {
        return;
    };

    // Add frame delta to accumulator, capped to prevent spiral of death.
    pw.accumulator += frame_dt.min(0.25);

    // If not enough time has accumulated for a single step, bail early.
    if pw.accumulator < pw.params.dt {
        world.insert_resource(pw);
        return;
    }

    // 1. Cleanup: remove bodies whose entities have been despawned.
    let dead: Vec<RigidBodyHandle> = pw
        .body_to_entity
        .iter()
        .filter(|(_h, e)| !world.is_alive(**e))
        .map(|(h, _e)| *h)
        .collect();
    for handle in dead {
        if let Some(entity) = pw.body_to_entity.remove(&handle) {
            pw.entity_to_body.remove(&entity.index());
        }
        pw.bodies.remove(
            handle,
            &mut pw.islands,
            &mut pw.colliders,
            &mut pw.impulse_joints,
            &mut pw.multibody_joints,
            true,
        );
    }

    // 2. Discover new rigid bodies (handle is None).
    {
        let mut new_bodies: Vec<(Entity, RigidBodyType3d, Vec3, Vec3, f32, f32, f32, bool, Vec3, Quat)> =
            Vec::new();
        world.query::<(&RigidBody3d, &Transform)>(|entity, (rb, tf)| {
            if rb.handle.is_none() {
                new_bodies.push((
                    entity,
                    rb.body_type,
                    rb.linear_velocity,
                    rb.angular_velocity,
                    rb.gravity_scale,
                    rb.linear_damping,
                    rb.angular_damping,
                    rb.ccd_enabled,
                    tf.translation,
                    tf.rotation,
                ));
            }
        });
        for (entity, body_type, linvel, angvel, grav, lindamp, angdamp, ccd, pos, rot) in
            new_bodies
        {
            let rb = RigidBodyBuilder::new(body_type_to_rapier(body_type))
                .translation(pos)
                .rotation(quat_to_scaled_axis(rot))
                .linvel(linvel)
                .angvel(angvel)
                .gravity_scale(grav)
                .linear_damping(lindamp)
                .angular_damping(angdamp)
                .ccd_enabled(ccd)
                .build();
            let handle = pw.bodies.insert(rb);
            pw.body_to_entity.insert(handle, entity);
            pw.entity_to_body.insert(entity.index(), handle);
            if let Some(comp) = world.get_mut::<RigidBody3d>(entity) {
                comp.handle = Some(handle);
            }
        }
    }

    // 3. Discover new colliders (handle is None, parent body already registered).
    {
        let mut new_colliders: Vec<(Entity, ColliderShape3d, f32, f32, f32, bool, RigidBodyHandle)> =
            Vec::new();
        world.query::<(&Collider3d, &RigidBody3d)>(|entity, (coll, rb)| {
            if coll.handle.is_none() {
                if let Some(body_handle) = rb.handle {
                    new_colliders.push((
                        entity,
                        coll.shape,
                        coll.restitution,
                        coll.friction,
                        coll.density,
                        coll.sensor,
                        body_handle,
                    ));
                }
            }
        });
        for (entity, shape, restitution, friction, density, sensor, body_handle) in new_colliders {
            let coll = shape_to_collider_builder(&shape)
                .restitution(restitution)
                .friction(friction)
                .density(density)
                .sensor(sensor)
                .build();
            let handle =
                pw.colliders
                    .insert_with_parent(coll, body_handle, &mut pw.bodies);
            if let Some(comp) = world.get_mut::<Collider3d>(entity) {
                comp.handle = Some(handle);
            }
        }
    }

    // 4. Sync kinematic bodies: push Transform → Rapier.
    {
        let mut kinematic_updates: Vec<(RigidBodyHandle, Vec3, Quat)> = Vec::new();
        world.query::<(&RigidBody3d, &Transform)>(|_entity, (rb, tf)| {
            if rb.body_type == RigidBodyType3d::KinematicPositionBased {
                if let Some(handle) = rb.handle {
                    kinematic_updates.push((handle, tf.translation, tf.rotation));
                }
            }
        });
        for (handle, pos, rot) in kinematic_updates {
            if let Some(body) = pw.bodies.get_mut(handle) {
                body.set_next_kinematic_position(Pose::from_parts(pos, rot));
            }
        }
    }

    // 5. Step the simulation with fixed dt, consuming the accumulator.
    let fixed_dt = pw.params.dt;
    while pw.accumulator >= fixed_dt {
        pw.pipeline.step(
            pw.gravity,
            &pw.params,
            &mut pw.islands,
            &mut pw.broad_phase,
            &mut pw.narrow_phase,
            &mut pw.bodies,
            &mut pw.colliders,
            &mut pw.impulse_joints,
            &mut pw.multibody_joints,
            &mut pw.ccd_solver,
            &(),
            &(),
        );
        pw.accumulator -= fixed_dt;
    }

    // 6. Sync dynamic/kinematic-velocity bodies: pull Rapier → Transform.
    {
        let mut sync_updates: Vec<(Entity, Vec3, Quat)> = Vec::new();
        world.query::<(&RigidBody3d,)>(|entity, (rb,)| {
            if rb.body_type == RigidBodyType3d::Dynamic
                || rb.body_type == RigidBodyType3d::KinematicVelocityBased
            {
                if let Some(handle) = rb.handle {
                    if let Some(body) = pw.bodies.get(handle) {
                        let pos = body.translation();
                        let rot = *body.rotation();
                        sync_updates.push((entity, pos, rot));
                    }
                }
            }
        });
        for (entity, pos, rot) in sync_updates {
            if let Some(tf) = world.get_mut::<Transform>(entity) {
                tf.translation = pos;
                tf.rotation = rot;
            }
        }
    }

    world.insert_resource(pw);
}

