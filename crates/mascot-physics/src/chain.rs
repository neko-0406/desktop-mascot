use glam::{Quat, Vec3};
use mascot_format::PhysBoneChain;

use crate::collider::WorldCollider;
use crate::verlet::{clamp_angle, solve_distance_constraint, VerletParticle};

/// Result of evaluating a bone's updated local rotation after physics simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoneRotationUpdate {
    pub bone_index: usize,
    pub local_rotation: Quat,
}

/// Represents a segment in a PhysBone chain corresponding to a skeleton bone.
#[derive(Debug, Clone)]
pub struct ChainSegment {
    pub bone_index: usize,
    pub parent_bone_index: i32,
    pub particle_a: usize, // Parent / start particle index in chain
    pub particle_b: usize, // Child / end particle index in chain
    pub rest_local_position: Vec3,
    pub rest_local_rotation: Quat,
    pub rest_length: f32,
    pub rest_direction: Vec3,
}

/// Simulation instance for a PhysBone chain.
#[derive(Debug, Clone)]
pub struct PhysBoneChainSim {
    pub root_bone_index: usize,
    pub pull: f32,
    pub spring: f32,
    pub damping: f32,
    pub stiffness: f32,
    pub gravity: Vec3,
    pub max_angle: f32,
    pub radius: f32,
    pub collider_indices: Vec<usize>,

    pub particles: Vec<VerletParticle>,
    pub segments: Vec<ChainSegment>,
}

impl PhysBoneChainSim {
    /// Creates a simulation chain from `PhysBoneChain` format and bone info.
    ///
    /// If the root bone has no children, a virtual tip particle is automatically
    /// constructed to allow the leaf bone to rotate naturally under physics.
    pub fn new(
        chain_def: &PhysBoneChain,
        root_bone_index: usize,
        root_parent_index: i32,
        root_rest_pos: Vec3,
        root_rest_rot: Quat,
        child_bones: &[(usize, Vec3, Quat)], // (child_bone_index, child_local_pos, child_local_rot)
    ) -> Self {
        let pull = chain_def.pull.clamp(0.0, 1.0);
        let spring = chain_def.spring.max(0.0);
        let damping = chain_def.damping.clamp(0.0, 1.0);
        let stiffness = chain_def.stiffness.clamp(0.0, 1.0);
        let gravity = Vec3::from_slice(&chain_def.gravity);
        let max_angle = chain_def.max_angle;
        let radius = chain_def.radius.max(0.001);

        let active_colliders = chain_def.collider_indices[..chain_def.collider_count as usize]
            .iter()
            .map(|&idx| idx as usize)
            .collect();

        let mut particles = Vec::new();
        let mut segments = Vec::new();

        // Particle 0: fixed at root bone position
        particles.push(VerletParticle::new(
            root_rest_pos,
            radius,
            true,
            Vec3::ZERO,
            0.0,
        ));

        if child_bones.is_empty() {
            // Leaf bone: add a virtual tip particle extending backward/downward along rest pose
            // or along local offset
            let tip_offset = root_rest_rot * Vec3::new(0.0, -0.15, -0.05);
            let tip_pos = root_rest_pos + tip_offset;
            let length = tip_offset.length().max(0.05);

            particles.push(VerletParticle::new(
                tip_pos,
                radius,
                false,
                tip_offset,
                length,
            ));

            segments.push(ChainSegment {
                bone_index: root_bone_index,
                parent_bone_index: root_parent_index,
                particle_a: 0,
                particle_b: 1,
                rest_local_position: root_rest_pos,
                rest_local_rotation: root_rest_rot,
                rest_length: length,
                rest_direction: tip_offset.normalize(),
            });
        } else {
            // Multi-bone chain
            let mut prev_particle_idx = 0;
            let mut prev_bone_idx = root_bone_index;
            let mut prev_parent_idx = root_parent_index;
            let mut prev_local_rot = root_rest_rot;
            let mut current_world_pos = root_rest_pos;

            for &(c_idx, c_pos, c_rot) in child_bones {
                let p_idx = particles.len();
                let length = c_pos.length().max(0.01);
                let world_pos = current_world_pos + c_pos;

                particles.push(VerletParticle::new(
                    world_pos,
                    radius,
                    false,
                    c_pos,
                    length,
                ));

                segments.push(ChainSegment {
                    bone_index: prev_bone_idx,
                    parent_bone_index: prev_parent_idx,
                    particle_a: prev_particle_idx,
                    particle_b: p_idx,
                    rest_local_position: c_pos,
                    rest_local_rotation: prev_local_rot,
                    rest_length: length,
                    rest_direction: c_pos.normalize(),
                });

                prev_particle_idx = p_idx;
                prev_bone_idx = c_idx;
                prev_parent_idx = prev_bone_idx as i32;
                prev_local_rot = c_rot;
                current_world_pos = world_pos;
            }

            // Virtual tip for the last bone
            let tip_offset = prev_local_rot * Vec3::new(0.0, -0.1, 0.0);
            let tip_pos = current_world_pos + tip_offset;
            let length = tip_offset.length().max(0.05);
            let p_tip = particles.len();

            particles.push(VerletParticle::new(
                tip_pos,
                radius,
                false,
                tip_offset,
                length,
            ));

            segments.push(ChainSegment {
                bone_index: prev_bone_idx,
                parent_bone_index: prev_parent_idx,
                particle_a: prev_particle_idx,
                particle_b: p_tip,
                rest_local_position: tip_offset,
                rest_local_rotation: prev_local_rot,
                rest_length: length,
                rest_direction: tip_offset.normalize(),
            });
        }

        Self {
            root_bone_index,
            pull,
            spring,
            damping,
            stiffness,
            gravity,
            max_angle,
            radius,
            collider_indices: active_colliders,
            particles,
            segments,
        }
    }

    /// Resets all particles to rest positions.
    pub fn reset_pose(&mut self, root_world_pos: Vec3, root_world_rot: Quat) {
        if self.particles.is_empty() {
            return;
        }

        self.particles[0].reset_position(root_world_pos);

        for seg in &self.segments {
            let p_a = self.particles[seg.particle_a].position;
            let dir = (root_world_rot * seg.rest_direction).normalize();
            let p_b = p_a + dir * seg.rest_length;
            self.particles[seg.particle_b].reset_position(p_b);
        }
    }

    /// Performs one physics step on this chain and returns reconstructed bone rotations.
    pub fn step(
        &mut self,
        dt: f32,
        external_accel: Vec3,
        root_world_pos: Vec3,
        root_world_rot: Quat,
        parent_world_rot: Quat,
        world_colliders: &[WorldCollider],
        constraint_iterations: usize,
    ) -> Vec<BoneRotationUpdate> {
        if self.particles.is_empty() || self.segments.is_empty() {
            return Vec::new();
        }

        // 1. Update root particle position (fixed to animated bone position)
        self.particles[0].position = root_world_pos;
        self.particles[0].prev_position = root_world_pos;

        // 2. Compute target positions for all particles based on animated/kinematic pose
        let mut target_positions = vec![Vec3::ZERO; self.particles.len()];
        target_positions[0] = root_world_pos;

        for seg in &self.segments {
            let p_a = target_positions[seg.particle_a];
            let dir = (root_world_rot * seg.rest_direction).normalize();
            target_positions[seg.particle_b] = p_a + dir * seg.rest_length;
        }

        // 3. Numerical integration & pull restoration
        let total_accel = self.gravity + external_accel;
        for i in 1..self.particles.len() {
            self.particles[i].integrate(total_accel, self.damping, dt);
            self.particles[i].apply_restoration(
                target_positions[i],
                self.pull,
                self.stiffness,
                dt,
            );
        }

        // 4. Constraint resolution
        for _ in 0..constraint_iterations.max(1) {
            for seg in &self.segments {
                let p_a = self.particles[seg.particle_a].position;
                let target_a = target_positions[seg.particle_a];
                let target_b = target_positions[seg.particle_b];
                let target_dir = (target_b - target_a).normalize();

                // (a) Distance constraint
                solve_distance_constraint(
                    p_a,
                    &mut self.particles[seg.particle_b].position,
                    seg.rest_length,
                    target_dir,
                );

                // (b) Angle limit constraint
                let current_dir = (self.particles[seg.particle_b].position - p_a).normalize();
                let clamped_dir = clamp_angle(current_dir, target_dir, self.max_angle);
                self.particles[seg.particle_b].position = p_a + clamped_dir * seg.rest_length;

                // (c) Collider collisions
                for &c_idx in &self.collider_indices {
                    if let Some(col) = world_colliders.get(c_idx) {
                        let radius = self.particles[seg.particle_b].radius;
                        col.resolve_collision(
                            &mut self.particles[seg.particle_b].position,
                            radius,
                        );
                    }
                }

                // Re-enforce distance constraint after collider push-out
                solve_distance_constraint(
                    p_a,
                    &mut self.particles[seg.particle_b].position,
                    seg.rest_length,
                    target_dir,
                );
            }
        }

        // 5. Reconstruct bone local rotation
        let mut updates = Vec::with_capacity(self.segments.len());
        let mut current_parent_world_rot = parent_world_rot;

        for seg in &self.segments {
            let p_a = self.particles[seg.particle_a].position;
            let p_b = self.particles[seg.particle_b].position;
            let sim_dir = (p_b - p_a).normalize();

            let target_a = target_positions[seg.particle_a];
            let target_b = target_positions[seg.particle_b];
            let rest_dir = (target_b - target_a).normalize();

            // Arc from target rest direction to simulated direction
            let rot_diff = if rest_dir.dot(sim_dir) < 0.99999 {
                let axis = rest_dir.cross(sim_dir);
                if axis.length_squared() > 1e-6 {
                    let angle = rest_dir.dot(sim_dir).clamp(-1.0, 1.0).acos();
                    Quat::from_axis_angle(axis.normalize(), angle)
                } else {
                    Quat::IDENTITY
                }
            } else {
                Quat::IDENTITY
            };

            let new_world_rot = (rot_diff * root_world_rot).normalize();
            let new_local_rot = (current_parent_world_rot.inverse() * new_world_rot).normalize();

            updates.push(BoneRotationUpdate {
                bone_index: seg.bone_index,
                local_rotation: new_local_rot,
            });

            current_parent_world_rot = new_world_rot;
        }

        updates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leaf_bone_chain_creation_and_step() {
        let chain_def = PhysBoneChain {
            root_bone_index: 5,
            pull: 0.2,
            spring: 0.5,
            damping: 0.1,
            stiffness: 0.0,
            gravity: [0.0, -9.81, 0.0],
            max_angle: std::f32::consts::FRAC_PI_2,
            radius: 0.02,
            collider_indices: [0; 8],
            collider_count: 0,
        };

        let mut sim = PhysBoneChainSim::new(
            &chain_def,
            5,
            4,
            Vec3::new(0.0, 1.45, -0.08),
            Quat::IDENTITY,
            &[],
        );

        assert_eq!(sim.particles.len(), 2);
        assert_eq!(sim.segments.len(), 1);

        // Step simulation for 5 frames
        let dt = 1.0 / 60.0;
        let mut updates = Vec::new();
        for _ in 0..5 {
            updates = sim.step(
                dt,
                Vec3::ZERO,
                Vec3::new(0.0, 1.45, -0.08),
                Quat::IDENTITY,
                Quat::IDENTITY,
                &[],
                2,
            );
        }

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].bone_index, 5);
        // Under negative Y gravity, the tip particle moves downward, producing a rotation
        assert!(sim.particles[1].position.y < 1.45);
    }
}
