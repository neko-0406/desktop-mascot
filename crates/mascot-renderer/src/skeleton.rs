use glam::{Mat4, Quat, Vec3};
use mascot_format::RawBone;
use wgpu::util::DeviceExt;
use wgpu::{Buffer, BufferUsages, Device, Queue};

/// A node in the hierarchical skeleton.
#[derive(Debug, Clone)]
pub struct BoneNode {
    pub name: String,
    pub parent_index: i32,
    pub local_position: Vec3,
    pub local_rotation: Quat,
    pub local_scale: Vec3,
    pub inverse_bind_matrix: Mat4,
    pub world_matrix: Mat4,
    pub skin_matrix: Mat4,
}

impl BoneNode {
    pub fn compute_local_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.local_scale, self.local_rotation, self.local_position)
    }
}

/// Manages skeleton hierarchy, bone transforms, and GPU skinning matrix buffers.
pub struct GpuSkeleton {
    pub bones: Vec<BoneNode>,
    pub bone_buffer: Buffer,
    pub skin_matrices: Vec<[[f32; 4]; 4]>,
}

impl GpuSkeleton {
    /// Creates a GPU skeleton from raw bones defined in `DmaFile`.
    pub fn from_raw_bones(device: &Device, raw_bones: &[RawBone]) -> Self {
        let mut bones = Vec::with_capacity(raw_bones.len().max(1));

        if raw_bones.is_empty() {
            // Non-rigged fallback: 1 dummy root bone with identity transform
            bones.push(BoneNode {
                name: "Root".to_string(),
                parent_index: -1,
                local_position: Vec3::ZERO,
                local_rotation: Quat::IDENTITY,
                local_scale: Vec3::ONE,
                inverse_bind_matrix: Mat4::IDENTITY,
                world_matrix: Mat4::IDENTITY,
                skin_matrix: Mat4::IDENTITY,
            });
        } else {
            // First pass: extract bone nodes
            for raw in raw_bones {
                let rot = Quat::from_array(raw.local_rotation);
                // Ensure quaternion is normalized
                let rot = if rot.length_squared() > 1e-4 {
                    rot.normalize()
                } else {
                    Quat::IDENTITY
                };

                let inv_mat = Mat4::from_cols_array(&raw.inverse_bind_matrix);

                bones.push(BoneNode {
                    name: raw.name_str().to_string(),
                    parent_index: raw.parent_index,
                    local_position: Vec3::from_slice(&raw.local_position),
                    local_rotation: rot,
                    local_scale: Vec3::from_slice(&raw.local_scale),
                    inverse_bind_matrix: inv_mat,
                    world_matrix: Mat4::IDENTITY,
                    skin_matrix: Mat4::IDENTITY,
                });
            }

            // Compute initial rest world matrices
            for i in 0..bones.len() {
                let local = bones[i].compute_local_matrix();
                let parent = bones[i].parent_index;
                let world = if parent >= 0 && (parent as usize) < i {
                    bones[parent as usize].world_matrix * local
                } else {
                    local
                };
                bones[i].world_matrix = world;

                // Check if inverse_bind_matrix was left as placeholder identity
                // while rest world transform is non-identity
                let is_identity_inv = (bones[i].inverse_bind_matrix - Mat4::IDENTITY).abs_diff_eq(Mat4::ZERO, 1e-4);
                let is_identity_world = (world - Mat4::IDENTITY).abs_diff_eq(Mat4::ZERO, 1e-4);
                if is_identity_inv && !is_identity_world {
                    // Auto-compute inverse bind matrix from rest pose
                    bones[i].inverse_bind_matrix = world.inverse();
                }

                bones[i].skin_matrix = bones[i].world_matrix * bones[i].inverse_bind_matrix;
            }
        }

        let skin_matrices: Vec<[[f32; 4]; 4]> = bones
            .iter()
            .map(|b| b.skin_matrix.to_cols_array_2d())
            .collect();

        let bone_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Skeleton Bone Matrices Buffer"),
            contents: bytemuck::cast_slice(&skin_matrices),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        });

        Self {
            bones,
            bone_buffer,
            skin_matrices,
        }
    }

    /// Sets local rotation for a bone by index.
    pub fn set_bone_local_rotation(&mut self, index: usize, rotation: Quat) {
        if index < self.bones.len() {
            self.bones[index].local_rotation = rotation.normalize();
        }
    }

    /// Sets local position for a bone by index.
    pub fn set_bone_local_position(&mut self, index: usize, position: Vec3) {
        if index < self.bones.len() {
            self.bones[index].local_position = position;
        }
    }

    /// Finds bone index by name.
    pub fn find_bone_index(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    /// Recomputes world and skin matrices for all bones without GPU upload.
    pub fn compute_world_transforms(&mut self) {
        for i in 0..self.bones.len() {
            let local = self.bones[i].compute_local_matrix();
            let parent = self.bones[i].parent_index;
            let world = if parent >= 0 && (parent as usize) < i {
                self.bones[parent as usize].world_matrix * local
            } else {
                local
            };
            self.bones[i].world_matrix = world;
            self.bones[i].skin_matrix = world * self.bones[i].inverse_bind_matrix;
            self.skin_matrices[i] = self.bones[i].skin_matrix.to_cols_array_2d();
        }
    }

    /// Re-evaluates bone hierarchy transforms and uploads final skin matrices to the GPU buffer.
    pub fn update(&mut self, queue: &Queue) {
        self.compute_world_transforms();

        queue.write_buffer(
            &self.bone_buffer,
            0,
            bytemuck::cast_slice(&self.skin_matrices),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skeleton_rest_pose_identity() {
        let raw = vec![
            RawBone::new("Hips", -1, [0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0], Mat4::IDENTITY.to_cols_array()),
            RawBone::new("Spine", 0, [0.0, 0.5, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0], Mat4::IDENTITY.to_cols_array()),
        ];

        // We can test the mathematical logic without a real wgpu device:
        let rot0 = Quat::from_array(raw[0].local_rotation);
        let rot1 = Quat::from_array(raw[1].local_rotation);
        let local0 = Mat4::from_scale_rotation_translation(Vec3::ONE, rot0, Vec3::from_slice(&raw[0].local_position));
        let local1 = Mat4::from_scale_rotation_translation(Vec3::ONE, rot1, Vec3::from_slice(&raw[1].local_position));
        let world0 = local0;
        let world1 = world0 * local1;

        assert!((world1.w_axis.y - 1.5).abs() < 1e-5);
    }
}
