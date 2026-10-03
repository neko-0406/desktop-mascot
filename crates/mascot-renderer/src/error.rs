use thiserror::Error;

#[derive(Error, Debug)]
pub enum RendererError {
    #[error("Failed to request graphics adapter")]
    AdapterNotFound,

    #[error("Failed to request graphics device: {0}")]
    DeviceRequestFailed(#[from] wgpu::RequestDeviceError),

    #[error("Surface error: {0}")]
    SurfaceError(String),

    #[error("Surface creation failed: {0}")]
    CreateSurfaceError(#[from] wgpu::CreateSurfaceError),

    #[error("Unsupported surface format")]
    UnsupportedSurfaceFormat,

    #[error("Mesh buffer error: {0}")]
    MeshError(String),
}
