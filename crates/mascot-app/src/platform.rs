use std::sync::Arc;
use tracing::info;
use winit::window::Window;

#[cfg(target_os = "windows")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Controls::MARGINS;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_TOPMOST,
};

/// Configure the OS native window for desktop mascot overlay:
/// - WS_EX_LAYERED and WS_EX_TOPMOST style
/// - DWM frame extension into client area (-1 margins) for complete alpha transparency
pub fn configure_transparent_window(window: &Arc<Window>) {
    #[cfg(target_os = "windows")]
    {
        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
                let hwnd = HWND(win32_handle.hwnd.get() as *mut _);
                unsafe {
                    let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                    let new_style =
                        ex_style | (WS_EX_LAYERED.0 as isize) | (WS_EX_TOPMOST.0 as isize);
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);

                    let margins = MARGINS {
                        cxLeftWidth: -1,
                        cxRightWidth: -1,
                        cyTopHeight: -1,
                        cyBottomHeight: -1,
                    };
                    if let Err(e) = DwmExtendFrameIntoClientArea(hwnd, &margins) {
                        tracing::warn!("Failed to extend DWM frame into client area: {:?}", e);
                    } else {
                        info!("Successfully applied DWM transparent margins and Win32 layered styles.");
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        info!("Non-Windows OS: standard transparent window configuration applied.");
    }
}
