use winit::event_loop::{ControlFlow, EventLoop};

use crate::app::App;

mod app;

fn main() {
    let event_loop = EventLoop::new().unwrap();

    // 常に画面を更新するモード
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::default();
    let _ = event_loop.run_app(&mut app);
}
