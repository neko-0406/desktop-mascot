//! mascot-renderer: wgpu-based avatar renderer for Desktop Mascot.

pub struct RendererStub {
    pub ready: bool,
}

impl Default for RendererStub {
    fn default() -> Self {
        Self { ready: true }
    }
}
