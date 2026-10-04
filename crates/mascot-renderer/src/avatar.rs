use std::sync::Arc;
use glam::Mat4;
use mascot_format::{DmaFile, TEXTURE_FORMAT_RAW_RGBA};
use wgpu::util::DeviceExt;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, Buffer, BufferUsages, Device,
    Queue,
};

use crate::error::RendererError;
use crate::material::{GpuMaterial, GpuTexture, LilToonUniform};
use crate::mesh::GpuMesh;
use crate::morph::MorphController;
use crate::pipeline::ModelUniform;
use crate::skeleton::GpuSkeleton;

/// Represents a loaded `.dma` avatar ready for GPU rendering with lilToon,
/// GPU skinning, and blendshape morph targets.
pub struct GpuAvatar {
    pub mesh: GpuMesh,
    pub skeleton: GpuSkeleton,
    pub morph_controller: MorphController,
    pub materials: Vec<GpuMaterial>,
    pub textures: Vec<Arc<GpuTexture>>,
    pub default_material: GpuMaterial,
    pub model_buffer: Buffer,
    pub model_bind_group: BindGroup,
}

pub type DmaModel = GpuAvatar;
pub type SkinnedModel = GpuAvatar;

impl GpuAvatar {
    /// Builds a `GpuAvatar` from parsed `DmaFile`.
    pub fn from_dma(
        device: &Device,
        queue: &Queue,
        dma: &DmaFile,
        material_layout: &BindGroupLayout,
        model_skeleton_layout: &BindGroupLayout,
    ) -> Result<Self, RendererError> {
        let dma_mesh = dma.mesh.as_ref().ok_or_else(|| {
            RendererError::ModelError("DMA file does not contain a mesh".to_string())
        })?;

        let mesh = GpuMesh::from_dma(device, dma_mesh);
        let skeleton = GpuSkeleton::from_raw_bones(device, &dma.skeleton);
        let morph_controller =
            MorphController::new(dma.morph_targets.clone(), dma_mesh.vertices.clone());

        // Default white fallback texture
        let default_white = GpuTexture::create_solid_color(
            device,
            queue,
            [255, 255, 255, 255],
            Some("Default White Diffuse"),
        );

        // Load textures
        let mut textures = Vec::with_capacity(dma.textures.len());
        for tex in &dma.textures {
            if tex.format == TEXTURE_FORMAT_RAW_RGBA
                && tex.data.len() == (tex.width * tex.height * 4) as usize
            {
                textures.push(Arc::new(GpuTexture::from_rgba(
                    device,
                    queue,
                    tex.width,
                    tex.height,
                    &tex.data,
                    Some(&tex.name),
                )));
            } else {
                // Compressed or placeholder fallback
                textures.push(default_white.clone());
            }
        }

        // Default fallback material
        let default_material = GpuMaterial::new(
            device,
            material_layout,
            "DefaultFallback".to_string(),
            LilToonUniform::default(),
            default_white.clone(),
        );

        // Load materials
        let mut materials = Vec::with_capacity(dma.materials.len().max(1));
        if dma.materials.is_empty() {
            materials.push(GpuMaterial::new(
                device,
                material_layout,
                "Default".to_string(),
                LilToonUniform::default(),
                default_white.clone(),
            ));
        } else {
            for mat in &dma.materials {
                let tex = if mat.params.base_texture_idx >= 0
                    && (mat.params.base_texture_idx as usize) < textures.len()
                {
                    textures[mat.params.base_texture_idx as usize].clone()
                } else {
                    default_white.clone()
                };

                let uniform = LilToonUniform::from(&mat.params);
                materials.push(GpuMaterial::new(
                    device,
                    material_layout,
                    mat.name.clone(),
                    uniform,
                    tex,
                ));
            }
        }

        // Create ModelUniform buffer
        let model_uniform = ModelUniform::default();
        let model_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Avatar Model Buffer"),
            contents: bytemuck::cast_slice(&[model_uniform]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        // Create Model & Skeleton BindGroup (Group 2)
        let model_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Avatar Model & Skeleton Bind Group (Group 2)"),
            layout: model_skeleton_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: model_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: skeleton.bone_buffer.as_entire_binding(),
                },
            ],
        });

        Ok(Self {
            mesh,
            skeleton,
            morph_controller,
            materials,
            textures,
            default_material,
            model_buffer,
            model_bind_group,
        })
    }

    /// Updates avatar transforms, skeletal bone hierarchy, and morph blendshapes.
    pub fn update(&mut self, queue: &Queue, model_matrix: Mat4) {
        let model_uniform = ModelUniform {
            model_matrix: model_matrix.to_cols_array_2d(),
            color: [1.0, 1.0, 1.0, 1.0],
        };
        queue.write_buffer(&self.model_buffer, 0, bytemuck::cast_slice(&[model_uniform]));

        self.skeleton.update(queue);
        self.morph_controller.apply(&self.mesh.vertex_buffer, queue);
    }
}
