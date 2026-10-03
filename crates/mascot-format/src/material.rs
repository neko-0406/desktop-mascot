use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct MaterialChunkHeader {
    pub material_count: u32,
    pub texture_count: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct LilToonMaterialParams {
    pub base_color: [f32; 4],            // Main diffuse RGBA
    pub shade_color: [f32; 4],           // 1st shade RGBA
    pub shade2_color: [f32; 4],          // 2nd shade RGBA
    pub shade_border: f32,               // 1st shade threshold (0.0 - 1.0)
    pub shade_blur: f32,                 // 1st shade blur width
    pub shade2_border: f32,              // 2nd shade threshold
    pub shade2_blur: f32,                // 2nd shade blur width

    // Outline
    pub outline_color: [f32; 4],         // Inverted hull outline RGBA
    pub outline_width: f32,              // Normal extrusion width in object units
    pub outline_enable: u32,             // 0: disabled, 1: enabled
    pub outline_vertex_color_blend: f32, // Blend ratio with vertex color thickness

    // Rimlight & Emission
    pub rim_color: [f32; 4],             // Rimlight color RGBA
    pub rim_border: f32,                 // Rim threshold
    pub rim_blur: f32,                   // Rim feathering
    pub emission_color: [f32; 4],        // Emission color RGBA

    // Texture indices (-1: no texture)
    pub base_texture_idx: i32,
    pub shade_texture_idx: i32,
    pub outline_texture_idx: i32,
    pub normal_texture_idx: i32,
}

impl Default for LilToonMaterialParams {
    fn default() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            shade_color: [0.85, 0.85, 0.9, 1.0],
            shade2_color: [0.7, 0.7, 0.75, 1.0],
            shade_border: 0.5,
            shade_blur: 0.1,
            shade2_border: 0.3,
            shade2_blur: 0.1,

            outline_color: [0.0, 0.0, 0.0, 1.0],
            outline_width: 1.0,
            outline_enable: 1,
            outline_vertex_color_blend: 0.0,

            rim_color: [1.0, 1.0, 1.0, 0.0],
            rim_border: 0.5,
            rim_blur: 0.2,
            emission_color: [0.0, 0.0, 0.0, 0.0],

            base_texture_idx: -1,
            shade_texture_idx: -1,
            outline_texture_idx: -1,
            normal_texture_idx: -1,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct RawMaterial {
    pub name: [u8; 64],
    pub params: LilToonMaterialParams,
}

impl RawMaterial {
    pub fn new(name: &str, params: LilToonMaterialParams) -> Self {
        let mut mat = Self {
            name: [0; 64],
            params,
        };
        mat.set_name(name);
        mat
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

pub const TEXTURE_FORMAT_PNG: u32 = 0;
pub const TEXTURE_FORMAT_RAW_RGBA: u32 = 1;
pub const TEXTURE_FORMAT_DDS: u32 = 2;
pub const TEXTURE_FORMAT_KTX2: u32 = 3;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct RawTextureHeader {
    pub name: [u8; 64],
    pub format: u32,
    pub width: u32,
    pub height: u32,
    pub data_length: u32,
}

impl RawTextureHeader {
    pub fn new(name: &str, format: u32, width: u32, height: u32, data_length: u32) -> Self {
        let mut hdr = Self {
            name: [0; 64],
            format,
            width,
            height,
            data_length,
        };
        hdr.set_name(name);
        hdr
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

/// High-level representation of a material.
#[derive(Debug, Clone, PartialEq)]
pub struct DmaMaterial {
    pub name: String,
    pub params: LilToonMaterialParams,
}

impl Default for DmaMaterial {
    fn default() -> Self {
        Self {
            name: "DefaultMaterial".to_string(),
            params: LilToonMaterialParams::default(),
        }
    }
}

/// High-level representation of an embedded texture.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DmaTexture {
    pub name: String,
    pub format: u32,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_material_sizes() {
        assert_eq!(std::mem::size_of::<LilToonMaterialParams>(), 148);
        assert_eq!(std::mem::size_of::<RawMaterial>(), 64 + 148);
        assert_eq!(std::mem::size_of::<RawTextureHeader>(), 64 + 16);
    }
}
