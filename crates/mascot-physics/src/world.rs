use glam::{Quat, Vec3};
use mascot_format::{PhysBoneChain, PhysCollider, RawBone};

use crate::chain::{BoneRotationUpdate, PhysBoneChainSim};
use crate::collider::{ColliderDef, WorldCollider};

/// Trait representing a readable view of bone transforms in the skeleton hierarchy.
pub trait BonePoseReader {
    fn bone_count(&self) -> usize;
    fn parent_index(&self, index: usize) -> i32;
    fn world_position(&self, index: usize) -> Vec3;
    fn world_rotation(&self, index: usize) -> Quat;
}

/// Standalone bone struct for testing or headless simulation.
#[derive(Debug, Clone)]
pub struct SimpleBone {
    pub name: String,
    pub parent_index: i32,
    pub local_position: Vec3,
    pub local_rotation: Quat,
    pub world_position: Vec3,
    pub world_rotation: Quat,
}

impl SimpleBone {
    pub fn new(name: &str, parent_index: i32, local_position: Vec3, local_rotation: Quat) -> Self {
        Self {
            name: name.to_string(),
            parent_index,
            local_position,
            local_rotation,
            world_position: local_position,
            world_rotation: local_rotation,
        }
    }
}

impl BonePoseReader for [SimpleBone] {
    fn bone_count(&self) -> usize {
        self.len()
    }

    fn parent_index(&self, index: usize) -> i32 {
        self.get(index).map(|b| b.parent_index).unwrap_or(-1)
    }

    fn world_position(&self, index: usize) -> Vec3 {
        self.get(index).map(|b| b.world_position).unwrap_or(Vec3::ZERO)
    }

    fn world_rotation(&self, index: usize) -> Quat {
        self.get(index).map(|b| b.world_rotation).unwrap_or(Quat::IDENTITY)
    }
}

impl BonePoseReader for Vec<SimpleBone> {
    fn bone_count(&self) -> usize {
        self.len()
    }

    fn parent_index(&self, index: usize) -> i32 {
        self.get(index).map(|b| b.parent_index).unwrap_or(-1)
    }

    fn world_position(&self, index: usize) -> Vec3 {
        self.get(index).map(|b| b.world_position).unwrap_or(Vec3::ZERO)
    }

    fn world_rotation(&self, index: usize) -> Quat {
        self.get(index).map(|b| b.world_rotation).unwrap_or(Quat::IDENTITY)
    }
}

/// Main physical simulation world managing PhysBone chains and collision objects.
#[derive(Debug, Clone)]
pub struct PhysicsWorld {
    pub chains: Vec<PhysBoneChainSim>,
    pub collider_defs: Vec<ColliderDef>,
    pub world_colliders: Vec<WorldCollider>,
    pub fixed_timestep: f32, // Default: 1.0 / 60.0
    pub substeps: u32,       // Default: 2
    pub constraint_iterations: usize, // Default: 2
    pub accumulated_time: f32,
    pub latest_updates: Vec<BoneRotationUpdate>,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self {
            chains: Vec::new(),
            collider_defs: Vec::new(),
            world_colliders: Vec::new(),
            fixed_timestep: 1.0 / 60.0,
            substeps: 2,
            constraint_iterations: 2,
            accumulated_time: 0.0,
            latest_updates: Vec::new(),
        }
    }
}

impl PhysicsWorld {
    /// Constructs a `PhysicsWorld` from format chunks and raw bones.
    pub fn from_format(
        chains: &[PhysBoneChain],
        colliders: &[PhysCollider],
        raw_bones: &[RawBone],
    ) -> Self {
        let mut collider_defs = Vec::with_capacity(colliders.len());
        let mut world_colliders = Vec::with_capacity(colliders.len());

        for c in colliders {
            let def = ColliderDef::from_format(c);
            let world_col = WorldCollider {
                shape: def.shape,
                world_center: def.local_position,
                world_rotation: def.local_rotation,
                radius: def.radius,
                height: def.height,
                inside_bounds: def.inside_bounds,
            };
            collider_defs.push(def);
            world_colliders.push(world_col);
        }

        let mut chain_sims = Vec::with_capacity(chains.len());

        for ch in chains {
            let root_idx = ch.root_bone_index as usize;
            if root_idx >= raw_bones.len() {
                continue;
            }

            let root_bone = &raw_bones[root_idx];
            let root_rest_pos = Vec3::from_slice(&root_bone.local_position);
            let rot = Quat::from_array(root_bone.local_rotation);
            let root_rest_rot = if rot.length_squared() > 1e-4 {
                rot.normalize()
            } else {
                Quat::IDENTITY
            };

            // Identify direct children in the hierarchy
            let mut child_bones = Vec::new();
            for (i, b) in raw_bones.iter().enumerate() {
                if b.parent_index == root_idx as i32 {
                    let b_pos = Vec3::from_slice(&b.local_position);
                    let b_rot = Quat::from_array(b.local_rotation).normalize();
                    child_bones.push((i, b_pos, b_rot));
                }
            }

            let sim = PhysBoneChainSim::new(
                ch,
                root_idx,
                root_bone.parent_index,
                root_rest_pos,
                root_rest_rot,
                &child_bones,
            );
            chain_sims.push(sim);
        }

        Self {
            chains: chain_sims,
            collider_defs,
            world_colliders,
            fixed_timestep: 1.0 / 60.0,
            substeps: 2,
            constraint_iterations: 2,
            accumulated_time: 0.0,
            latest_updates: Vec::new(),
        }
    }

    /// Steps the physics simulation forward in time.
    ///
    /// Updates all world colliders and advances each PhysBone chain using fixed-timestep substeps.
    /// Returns the bone rotation updates that should be applied to the skeleton.
    pub fn step<B: BonePoseReader>(
        &mut self,
        dt: f32,
        bones: &B,
        external_accel: Vec3,
    ) -> &[BoneRotationUpdate] {
        // 1. Update world colliders from current bone world poses
        for (i, def) in self.collider_defs.iter().enumerate() {
            let bone_pos = bones.world_position(def.root_bone_index);
            let bone_rot = bones.world_rotation(def.root_bone_index);
            self.world_colliders[i].update_from_bone(bone_pos, bone_rot, def);
        }

        // 2. Accumulate delta time and simulate with fixed substeps
        let clamped_dt = dt.clamp(0.0001, 0.1);
        self.accumulated_time += clamped_dt;

        self.latest_updates.clear();

        while self.accumulated_time >= self.fixed_timestep {
            self.accumulated_time -= self.fixed_timestep;
            let substep_dt = self.fixed_timestep / (self.substeps as f32);

            for _ in 0..self.substeps {
                for chain in &mut self.chains {
                    let root_pos = bones.world_position(chain.root_bone_index);
                    let root_rot = bones.world_rotation(chain.root_bone_index);
                    let parent_idx = bones.parent_index(chain.root_bone_index);
                    let parent_rot = if parent_idx >= 0 {
                        bones.world_rotation(parent_idx as usize)
                    } else {
                        Quat::IDENTITY
                    };

                    let updates = chain.step(
                        substep_dt,
                        external_accel,
                        root_pos,
                        root_rot,
                        parent_rot,
                        &self.world_colliders,
                        self.constraint_iterations,
                    );

                    self.latest_updates = updates;
                }
            }
        }

        &self.latest_updates
    }

    /// Direct slice-based step helper (convenience for caller without trait implementation).
    pub fn step_raw(
        &mut self,
        dt: f32,
        bone_world_positions: &[Vec3],
        bone_world_rotations: &[Quat],
        bone_parent_indices: &[i32],
        external_accel: Vec3,
    ) -> &[BoneRotationUpdate] {
        struct RawPoseView<'a> {
            positions: &'a [Vec3],
            rotations: &'a [Quat],
            parents: &'a [i32],
        }

        impl<'a> BonePoseReader for RawPoseView<'a> {
            fn bone_count(&self) -> usize {
                self.positions.len()
            }
            fn parent_index(&self, index: usize) -> i32 {
                self.parents.get(index).copied().unwrap_or(-1)
            }
            fn world_position(&self, index: usize) -> Vec3 {
                self.positions.get(index).copied().unwrap_or(Vec3::ZERO)
            }
            fn world_rotation(&self, index: usize) -> Quat {
                self.rotations.get(index).copied().unwrap_or(Quat::IDENTITY)
            }
        }

        let view = RawPoseView {
            positions: bone_world_positions,
            rotations: bone_world_rotations,
            parents: bone_parent_indices,
        };

        self.step(dt, &view, external_accel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_physics_world_end_to_end() {
        let mut ch = PhysBoneChain::default();
        ch.root_bone_index = 1;
        ch.pull = 0.2;
        ch.gravity = [0.0, -9.81, 0.0];
        ch.collider_count = 1;
        ch.collider_indices[0] = 0;

        let col = PhysCollider::sphere(0, [0.0, 0.0, 0.0], 0.1);

        let bones = vec![
            SimpleBone::new("Head", -1, Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY),
            SimpleBone::new("Hair", 0, Vec3::new(0.0, 1.0, -0.05), Quat::IDENTITY),
        ];

        let mut world = PhysicsWorld::from_format(&[ch], &[col], &[]);
        // Since raw_bones was empty in constructor above, manually add chain:
        let sim = PhysBoneChainSim::new(
            &ch,
            1,
            0,
            Vec3::new(0.0, 1.0, -0.05),
            Quat::IDENTITY,
            &[],
        );
        world.chains.push(sim);

        // Step simulation for 0.1 seconds
        let updates = world.step(0.1, &bones, Vec3::ZERO);
        assert!(!updates.is_empty());
        assert_eq!(updates[0].bone_index, 1);
    }
}
