use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct MorphChunkHeader {
    pub morph_target_count: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct MorphTargetHeader {
    pub name: [u8; 32],
    pub delta_count: u32,
}

impl MorphTargetHeader {
    pub fn new(name: &str, delta_count: u32) -> Self {
        let mut hdr = Self {
            name: [0; 32],
            delta_count,
        };
        hdr.set_name(name);
        hdr
    }

    pub fn name_str(&self) -> &str {
        let len = self.name.iter().position(|&c| c == 0).unwrap_or(self.name.len());
        std::str::from_utf8(&self.name[..len]).unwrap_or("<invalid utf-8>")
    }

    pub fn set_name(&mut self, s: &str) {
        self.name = [0; 32];
        let bytes = s.as_bytes();
        let copy_len = bytes.len().min(31);
        self.name[..copy_len].copy_from_slice(&bytes[..copy_len]);
    }
}

/// Sparse blendshape delta for a single vertex (40 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct DeltaVertex {
    pub vertex_index: u32,
    pub delta_position: [f32; 3],
    pub delta_normal: [f32; 3],
    pub delta_tangent: [f32; 3],
}

impl Default for DeltaVertex {
    fn default() -> Self {
        Self {
            vertex_index: 0,
            delta_position: [0.0; 3],
            delta_normal: [0.0; 3],
            delta_tangent: [0.0; 3],
        }
    }
}

/// High-level representation of a morph target (blendshape).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MorphTarget {
    pub name: String,
    pub deltas: Vec<DeltaVertex>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_morph_sizes() {
        assert_eq!(std::mem::size_of::<MorphChunkHeader>(), 8);
        assert_eq!(std::mem::size_of::<MorphTargetHeader>(), 36);
        assert_eq!(std::mem::size_of::<DeltaVertex>(), 40);

        let target = MorphTargetHeader::new("vrc.blink", 10);
        assert_eq!(target.name_str(), "vrc.blink");
        assert_eq!(target.delta_count, 10);
    }
}
