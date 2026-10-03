use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct SkeletonChunkHeader {
    pub bone_count: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct RawBone {
    pub name: [u8; 64],                 // UTF-8 NULL-terminated string
    pub parent_index: i32,              // Root is -1
    pub local_position: [f32; 3],       // Local position relative to parent
    pub local_rotation: [f32; 4],       // Local rotation quaternion (x, y, z, w)
    pub local_scale: [f32; 3],          // Local scale (x, y, z)
    pub inverse_bind_matrix: [f32; 16], // 4x4 inverse bind matrix (column-major)
}

impl Default for RawBone {
    fn default() -> Self {
        let mut identity = [0.0f32; 16];
        identity[0] = 1.0;
        identity[5] = 1.0;
        identity[10] = 1.0;
        identity[15] = 1.0;

        Self {
            name: [0; 64],
            parent_index: -1,
            local_position: [0.0, 0.0, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0, 1.0, 1.0],
            inverse_bind_matrix: identity,
        }
    }
}

impl RawBone {
    pub fn new(
        name: &str,
        parent_index: i32,
        local_position: [f32; 3],
        local_rotation: [f32; 4],
        local_scale: [f32; 3],
        inverse_bind_matrix: [f32; 16],
    ) -> Self {
        let mut bone = Self {
            name: [0; 64],
            parent_index,
            local_position,
            local_rotation,
            local_scale,
            inverse_bind_matrix,
        };
        bone.set_name(name);
        bone
    }

    pub fn name_str(&self) -> &str {
        let len = self.name.iter().position(|&c| c == 0).unwrap_or(self.name.len());
        std::str::from_utf8(&self.name[..len]).unwrap_or("<invalid utf-8>")
    }

    pub fn set_name(&mut self, s: &str) {
        self.name = [0; 64];
        let bytes = s.as_bytes();
        let copy_len = bytes.len().min(63);
        self.name[..copy_len].copy_from_slice(&bytes[..copy_len]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_bone_size_and_name() {
        assert_eq!(std::mem::size_of::<RawBone>(), 172);
        let mut bone = RawBone::default();
        bone.set_name("Hips_Bone");
        assert_eq!(bone.name_str(), "Hips_Bone");
    }
}
