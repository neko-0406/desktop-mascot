use winit::{
    application::ApplicationHandler, event::WindowEvent,
    platform::windows::WindowAttributesExtWindows, window::Window,
};

#[derive(Default)]
pub struct App {
    window: Option<Window>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window_attributes = Window::default_attributes()
            .with_title("デスクトップマスコット") // windowタイトル
            .with_skip_taskbar(true) // タスクバーへの表示をしない
            // .with_transparent(true)                    // 透過設定
            // .with_decorations(false)                      // window枠など
            .with_window_level(winit::window::WindowLevel::AlwaysOnTop);

        // イベントループの作成
        self.window = Some(
            event_loop
                .create_window(window_attributes)
                .expect("イベントループの作成に失敗しました"),
        );
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                println!("Windowのクローズボタンが押されました");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.window.as_ref().unwrap().request_redraw();
            }
            _ => {}
        }
    }
}
