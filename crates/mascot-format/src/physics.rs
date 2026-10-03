use bytemuck::{Pod, Zeroable};

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColliderShape {
    Sphere = 0,
    Capsule = 1,
    Plane = 2,
}

impl TryFrom<u32> for ColliderShape {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Sphere),
            1 => Ok(Self::Capsule),
            2 => Ok(Self::Plane),
            other => Err(other),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct PhysChunkHeader {
    pub chain_count: u32,
    pub collider_count: u32,
}

/// PhysBone spring/mass chain parameters (76 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct PhysBoneChain {
    pub root_bone_index: u32,       // Index into skeleton bone list
    pub pull: f32,                  // Spring restitution to rest pose (0.0 - 1.0)
    pub spring: f32,                // Elastic oscillations
    pub damping: f32,               // Velocity dissipation rate (0.0 - 1.0)
    pub stiffness: f32,             // Resistance to external deformation
    pub gravity: [f32; 3],          // Directional gravity vector
    pub max_angle: f32,             // Angular limit cone in radians
    pub radius: f32,                // Particle collision thickness
    pub collider_indices: [u32; 8], // Indices of active colliders (up to 8)
    pub collider_count: u32,        // Active collider count (0..=8)
}

impl Default for PhysBoneChain {
    fn default() -> Self {
        Self {
            root_bone_index: 0,
            pull: 0.2,
            spring: 0.8,
            damping: 0.1,
            stiffness: 0.0,
            gravity: [0.0, -9.81, 0.0],
            max_angle: std::f32::consts::FRAC_PI_2,
            radius: 0.02,
            collider_indices: [0; 8],
            collider_count: 0,
        }
    }
}

/// Dynamic physical collision body (52 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct PhysCollider {
    pub shape: u32,           // 0: Sphere, 1: Capsule, 2: Plane
    pub root_bone_index: u32, // Attached bone index
    pub position: [f32; 3],   // Local offset relative to bone
    pub rotation: [f32; 4],   // Local rotation quaternion (x, y, z, w)
    pub radius: f32,          // Sphere/Capsule radius
    pub height: f32,          // Capsule height / distance between sphere caps
    pub inside_bounds: u32,   // 0: outside bounce, 1: inside containment
    pub reserved: u32,
}

impl Default for PhysCollider {
    fn default() -> Self {
        Self {
            shape: ColliderShape::Sphere as u32,
            root_bone_index: 0,
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            radius: 0.1,
            height: 0.0,
            inside_bounds: 0,
            reserved: 0,
        }
    }
}

impl PhysCollider {
    pub fn sphere(root_bone_index: u32, position: [f32; 3], radius: f32) -> Self {
        Self {
            shape: ColliderShape::Sphere as u32,
            root_bone_index,
            position,
            rotation: [0.0, 0.0, 0.0, 1.0],
            radius,
            height: 0.0,
            inside_bounds: 0,
            reserved: 0,
        }
    }

    pub fn capsule(
        root_bone_index: u32,
        position: [f32; 3],
        rotation: [f32; 4],
        radius: f32,
        height: f32,
    ) -> Self {
        Self {
            shape: ColliderShape::Capsule as u32,
            root_bone_index,
            position,
            rotation,
            radius,
            height,
            inside_bounds: 0,
            reserved: 0,
        }
    }

    pub fn plane(root_bone_index: u32, position: [f32; 3], rotation: [f32; 4]) -> Self {
        Self {
            shape: ColliderShape::Plane as u32,
            root_bone_index,
            position,
            rotation,
            radius: 0.0,
            height: 0.0,
            inside_bounds: 0,
            reserved: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phys_struct_sizes() {
        assert_eq!(std::mem::size_of::<PhysChunkHeader>(), 8);
        assert_eq!(std::mem::size_of::<PhysBoneChain>(), 76);
        assert_eq!(std::mem::size_of::<PhysCollider>(), 52);
    }
}
