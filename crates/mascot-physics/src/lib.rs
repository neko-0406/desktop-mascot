//! mascot-physics: PhysBone Verlet physics engine for Desktop Mascot.

pub struct PhysicsStub {
    pub ready: bool,
}

impl Default for PhysicsStub {
    fn default() -> Self {
        Self { ready: true }
    }
}
