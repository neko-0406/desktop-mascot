//! mascot-physics: VRC PhysBone compliant Verlet physics engine & procedural motion for Desktop Mascot.

pub mod chain;
pub mod collider;
pub mod motion;
pub mod verlet;
pub mod world;

pub use chain::{BoneRotationUpdate, ChainSegment, PhysBoneChainSim};
pub use collider::{
    closest_point_on_segment, resolve_capsule, resolve_plane, resolve_sphere, ColliderDef,
    WorldCollider,
};
pub use motion::{
    smooth_damp, BlinkController, BreathingController, BreathingPose, LookAtController,
    LookAtPose,
};
pub use verlet::{
    clamp_angle, solve_angle_limit, solve_distance_constraint, VerletParticle,
};
pub use world::{BonePoseReader, PhysicsWorld, SimpleBone};

/// Legacy stub kept for backward compatibility if referenced.
pub struct PhysicsStub {
    pub ready: bool,
}

impl Default for PhysicsStub {
    fn default() -> Self {
        Self { ready: true }
    }
}
