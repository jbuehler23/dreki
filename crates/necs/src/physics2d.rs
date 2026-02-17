//! 2D physics integration via Rapier.
//!
//! Provides rigid-body and collider components that automatically synchronize
//! with an internal Rapier simulation. Add [`PhysicsWorld2d`] as a resource,
//! attach [`RigidBody2d`] and [`Collider2d`] to your entities, and run
//! [`physics_step_2d`] each frame.

use std::collections::HashMap;

use rapier2d::prelude::*;

use crate::ecs::{Entity, World};
use crate::math::{Quat, Transform};

// ── Conversion helpers ──────────────────────────────────────────────────

fn body_type_to_rapier(bt: RigidBodyType2d) -> RigidBodyType {
    match bt {
        RigidBodyType2d::Dynamic => RigidBodyType::Dynamic,
        RigidBodyType2d::Fixed => RigidBodyType::Fixed,
        RigidBodyType2d::KinematicPositionBased => RigidBodyType::KinematicPositionBased,
        RigidBodyType2d::KinematicVelocityBased => RigidBodyType::KinematicVelocityBased,
    }
}

fn shape_to_collider_builder(shape: &ColliderShape2d) -> ColliderBuilder {
    match *shape {
        ColliderShape2d::Ball { radius } => ColliderBuilder::ball(radius),
        ColliderShape2d::Cuboid { hx, hy } => ColliderBuilder::cuboid(hx, hy),
        ColliderShape2d::CapsuleY {
            half_height,
            radius,
        } => ColliderBuilder::capsule_y(half_height, radius),
        ColliderShape2d::CapsuleX {
            half_height,
            radius,
        } => ColliderBuilder::capsule_x(half_height, radius),
    }
}

fn quat_to_angle(q: Quat) -> f32 {
    let (z, _y, _x) = q.to_euler(glam::EulerRot::ZYX);
    z
}

fn angle_to_quat(angle: f32) -> Quat {
    Quat::from_rotation_z(angle)
}

// ── Components ──────────────────────────────────────────────────────────

/// Rigid-body type for 2D physics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigidBodyType2d {
    Dynamic,
    Fixed,
    KinematicPositionBased,
    KinematicVelocityBased,
}

/// A 2D rigid-body component.
///
/// Attach to an entity alongside a [`Transform`] to give it physical behaviour.
/// The Rapier handle is managed internally — you only set the descriptive fields.
#[derive(Debug, Clone)]
pub struct RigidBody2d {
    pub body_type: RigidBodyType2d,
    pub linear_velocity: Vec2,
    pub angular_velocity: f32,
    pub gravity_scale: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub ccd_enabled: bool,
    pub(crate) handle: Option<RigidBodyHandle>,
}

impl RigidBody2d {
    /// A dynamic body affected by gravity and forces.
    pub fn dynamic() -> Self {
        Self {
            body_type: RigidBodyType2d::Dynamic,
            linear_velocity: Vec2::ZERO,
            angular_velocity: 0.0,
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
            body_type: RigidBodyType2d::Fixed,
            ..Self::dynamic()
        }
    }

    /// A kinematic body driven by position updates.
    pub fn kinematic_position() -> Self {
        Self {
            body_type: RigidBodyType2d::KinematicPositionBased,
            ..Self::dynamic()
        }
    }

    /// A kinematic body driven by velocity.
    pub fn kinematic_velocity() -> Self {
        Self {
            body_type: RigidBodyType2d::KinematicVelocityBased,
            ..Self::dynamic()
        }
    }

    pub fn with_linear_velocity(mut self, v: Vec2) -> Self {
        self.linear_velocity = v;
        self
    }

    pub fn with_angular_velocity(mut self, v: f32) -> Self {
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

/// Collider shape for 2D physics.
#[derive(Debug, Clone, Copy)]
pub enum ColliderShape2d {
    Ball { radius: f32 },
    Cuboid { hx: f32, hy: f32 },
    CapsuleY { half_height: f32, radius: f32 },
    CapsuleX { half_height: f32, radius: f32 },
}

/// A 2D collider component.
///
/// Attach alongside a [`RigidBody2d`]. The Rapier handle is managed internally.
#[derive(Debug, Clone)]
pub struct Collider2d {
    pub shape: ColliderShape2d,
    pub restitution: f32,
    pub friction: f32,
    pub density: f32,
    pub sensor: bool,
    pub(crate) handle: Option<ColliderHandle>,
}

impl Collider2d {
    /// A circular collider.
    pub fn ball(radius: f32) -> Self {
        Self {
            shape: ColliderShape2d::Ball { radius },
            restitution: 0.0,
            friction: 0.5,
            density: 1.0,
            sensor: false,
            handle: None,
        }
    }

    /// A rectangular collider (half-extents).
    pub fn cuboid(hx: f32, hy: f32) -> Self {
        Self {
            shape: ColliderShape2d::Cuboid { hx, hy },
            ..Self::ball(0.0)
        }
    }

    /// A vertical capsule collider.
    pub fn capsule_y(half_height: f32, radius: f32) -> Self {
        Self {
            shape: ColliderShape2d::CapsuleY {
                half_height,
                radius,
            },
            ..Self::ball(0.0)
        }
    }

    /// A horizontal capsule collider.
    pub fn capsule_x(half_height: f32, radius: f32) -> Self {
        Self {
            shape: ColliderShape2d::CapsuleX {
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

/// The 2D physics world. Insert as a resource and run [`physics_step_2d`] each frame.
pub struct PhysicsWorld2d {
    gravity: Vec2,
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

impl std::fmt::Debug for PhysicsWorld2d {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysicsWorld2d")
            .field("gravity", &self.gravity)
            .field("bodies", &self.bodies.len())
            .field("colliders", &self.colliders.len())
            .finish()
    }
}

impl PhysicsWorld2d {
    /// Create a new physics world with default gravity (0, -9.81).
    pub fn new() -> Self {
        Self {
            gravity: Vec2::new(0.0, -9.81),
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
    pub fn with_gravity(mut self, g: Vec2) -> Self {
        self.gravity = g;
        self
    }
}

impl Default for PhysicsWorld2d {
    fn default() -> Self {
        Self::new()
    }
}

// ── Plugin ──────────────────────────────────────────────────────────────

/// Plugin that registers the 2D physics resource and update system.
///
/// # Example
///
/// ```ignore
/// Game::new("My Game")
///     .plugin(Physics2d)
///     .setup(setup)
///     .run();
/// ```
pub struct Physics2d;

impl crate::game::Plugin for Physics2d {
    fn build(&self, game: &mut crate::game::Game) {
        game.insert_resource(PhysicsWorld2d::new());
        game.add_update_system(|ctx| physics_step_2d(&mut ctx.world));
    }
}

// ── System ──────────────────────────────────────────────────────────────

/// Advance the 2D physics simulation by one frame.
///
/// Uses the extract/reinsert pattern to borrow the physics world and the ECS
/// world simultaneously. Physics runs with a fixed timestep (default 1/60s)
/// using an accumulator to decouple simulation from frame rate.
pub(crate) fn physics_step_2d(world: &mut World) {
    let frame_dt = world.resource::<crate::time::Time>().delta_secs();
    if frame_dt <= 0.0 {
        return;
    }

    let Some(mut pw) = world.resource_remove::<PhysicsWorld2d>() else {
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
        let mut new_bodies: Vec<(Entity, RigidBodyType2d, Vec2, f32, f32, f32, f32, bool, Vec2, f32)> =
            Vec::new();
        world.query::<(&RigidBody2d, &Transform)>(|entity, (rb, tf)| {
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
                    Vec2::new(tf.translation.x, tf.translation.y),
                    quat_to_angle(tf.rotation),
                ));
            }
        });
        for (entity, body_type, linvel, angvel, grav, lindamp, angdamp, ccd, pos, angle) in
            new_bodies
        {
            let rb = RigidBodyBuilder::new(body_type_to_rapier(body_type))
                .translation(pos)
                .rotation(angle)
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
            if let Some(comp) = world.get_mut::<RigidBody2d>(entity) {
                comp.handle = Some(handle);
            }
        }
    }

    // 3. Discover new colliders (handle is None, parent body already registered).
    {
        let mut new_colliders: Vec<(Entity, ColliderShape2d, f32, f32, f32, bool, RigidBodyHandle)> =
            Vec::new();
        world.query::<(&Collider2d, &RigidBody2d)>(|entity, (coll, rb)| {
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
            if let Some(comp) = world.get_mut::<Collider2d>(entity) {
                comp.handle = Some(handle);
            }
        }
    }

    // 4. Sync kinematic bodies: push Transform → Rapier.
    {
        let mut kinematic_updates: Vec<(RigidBodyHandle, Vec2, f32)> = Vec::new();
        world.query::<(&RigidBody2d, &Transform)>(|_entity, (rb, tf)| {
            if rb.body_type == RigidBodyType2d::KinematicPositionBased {
                if let Some(handle) = rb.handle {
                    kinematic_updates.push((
                        handle,
                        Vec2::new(tf.translation.x, tf.translation.y),
                        quat_to_angle(tf.rotation),
                    ));
                }
            }
        });
        for (handle, pos, angle) in kinematic_updates {
            if let Some(body) = pw.bodies.get_mut(handle) {
                body.set_next_kinematic_position(Pose::new(pos, angle));
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
        let mut sync_updates: Vec<(Entity, Vec2, f32)> = Vec::new();
        world.query::<(&RigidBody2d,)>(|entity, (rb,)| {
            if rb.body_type == RigidBodyType2d::Dynamic
                || rb.body_type == RigidBodyType2d::KinematicVelocityBased
            {
                if let Some(handle) = rb.handle {
                    if let Some(body) = pw.bodies.get(handle) {
                        let pos = body.translation();
                        let angle = body.rotation().angle();
                        sync_updates.push((entity, pos, angle));
                    }
                }
            }
        });
        for (entity, pos, angle) in sync_updates {
            if let Some(tf) = world.get_mut::<Transform>(entity) {
                tf.translation.x = pos.x;
                tf.translation.y = pos.y;
                tf.rotation = angle_to_quat(angle);
            }
        }
    }

    world.insert_resource(pw);
}

