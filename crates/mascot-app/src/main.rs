mod app;
mod platform;

use app::MascotApp;
use tracing::info;
use winit::event_loop::EventLoop;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,mascot_app=debug,mascot_renderer=debug".into()),
        )
        .init();

    info!("Starting Desktop Mascot runtime (Phase 2 Window & Renderer)...");

    let event_loop = EventLoop::new()?;
    let mut app = MascotApp::default();
    event_loop.run_app(&mut app)?;

    info!("Desktop Mascot terminated cleanly.");
    Ok(())
}
