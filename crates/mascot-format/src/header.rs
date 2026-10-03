use bytemuck::{Pod, Zeroable};

pub const DMA_MAGIC: [u8; 4] = *b"DMA1";
pub const DMA_VERSION: u32 = 1;

pub const FLAG_COMPRESSED: u32 = 1 << 0;
pub const FLAG_LITTLE_ENDIAN: u32 = 1 << 1;

pub const CHUNK_META: [u8; 4] = *b"META";
pub const CHUNK_SKEL: [u8; 4] = *b"SKEL";
pub const CHUNK_MESH: [u8; 4] = *b"MESH";
pub const CHUNK_MORPH: [u8; 4] = *b"MORP";
pub const CHUNK_MAT: [u8; 4] = *b"MAT ";
pub const CHUNK_PHYS: [u8; 4] = *b"PHYS";
pub const CHUNK_ANIM: [u8; 4] = *b"ANIM";

/// Fixed 32-byte header at the start of a `.dma` file.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct DmaHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub flags: u32,
    pub chunk_count: u32,
    pub total_file_size: u64,
    pub reserved: [u8; 8],
}

impl Default for DmaHeader {
    fn default() -> Self {
        Self {
            magic: DMA_MAGIC,
            version: DMA_VERSION,
            flags: FLAG_LITTLE_ENDIAN,
            chunk_count: 0,
            total_file_size: 32,
            reserved: [0; 8],
        }
    }
}

/// Table of contents entry for a single chunk. (24 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct TocEntry {
    pub chunk_type: [u8; 4],
    pub reserved: u32,
    pub offset: u64,
    pub length: u64,
}

impl TocEntry {
    pub fn new(chunk_type: [u8; 4], offset: u64, length: u64) -> Self {
        Self {
            chunk_type,
            reserved: 0,
            offset,
            length,
        }
    }

    pub fn type_name(&self) -> String {
        String::from_utf8_lossy(&self.chunk_type).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_size() {
        assert_eq!(std::mem::size_of::<DmaHeader>(), 32);
        assert_eq!(std::mem::size_of::<TocEntry>(), 24);
    }
}
