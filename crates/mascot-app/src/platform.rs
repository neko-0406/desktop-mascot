use std::sync::Arc;
use tracing::info;
use winit::window::Window;

use crate::hittest::HitTester;

#[cfg(target_os = "windows")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, ScreenToClient, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Controls::MARGINS;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, HTCLIENT, HTTRANSPARENT,
    SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW, WM_NCDESTROY,
    WM_NCHITTEST, WS_EX_LAYERED, WS_EX_TOPMOST,
};

const SUBCLASS_ID_HITTEST: usize = 0x4D53_4354; // 'MSCT'

/// 2D rectangle representing desktop work area boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkAreaRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WorkAreaRect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    #[allow(dead_code)]
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

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

/// Installs Win32 WM_NCHITTEST subclassing to enable click-through on transparent pixels
/// while capturing mouse events over mascot geometry and UI elements.
pub fn install_hit_test_subclass(window: &Arc<Window>, hit_tester: Arc<HitTester>) {
    #[cfg(target_os = "windows")]
    {
        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
                let hwnd = HWND(win32_handle.hwnd.get() as *mut _);
                let ref_data = Arc::into_raw(hit_tester) as usize;
                unsafe {
                    let success = SetWindowSubclass(
                        hwnd,
                        Some(mascot_subclass_proc),
                        SUBCLASS_ID_HITTEST,
                        ref_data,
                    );
                    if success.as_bool() {
                        info!("Installed Win32 WM_NCHITTEST click-through subclass procedure.");
                    } else {
                        tracing::warn!("Failed to install Win32 window subclass.");
                        // Reclaim arc to avoid leak on failure
                        let _ = Arc::from_raw(ref_data as *const HitTester);
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, hit_tester);
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn mascot_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    uid_subclass: usize,
    ref_data: usize,
) -> LRESULT {
    if ref_data == 0 {
        return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    }

    match msg {
        WM_NCHITTEST => {
            // Screen cursor coordinates from lParam (LOWORD = X, HIWORD = Y)
            let raw_x = (lparam.0 as i32 & 0xFFFF) as i16 as i32;
            let raw_y = ((lparam.0 as i32 >> 16) & 0xFFFF) as i16 as i32;
            let mut pt = POINT { x: raw_x, y: raw_y };

            // Convert to window client coordinates
            let _ = unsafe { ScreenToClient(hwnd, &mut pt) };

            let hit_tester = unsafe { &*(ref_data as *const HitTester) };
            if hit_tester.is_hit(pt.x as f32, pt.y as f32) {
                // Inside mascot geometry or egui UI: capture mouse clicks
                LRESULT(HTCLIENT as isize)
            } else {
                // Transparent area: pass mouse input through to underlying application
                LRESULT(HTTRANSPARENT as isize)
            }
        }
        WM_NCDESTROY => {
            // Clean up subclass and safely drop Arc
            unsafe {
                let _ = RemoveWindowSubclass(hwnd, Some(mascot_subclass_proc), uid_subclass);
                let _ = Arc::from_raw(ref_data as *const HitTester);
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
        }
        _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

/// Retrieves the desktop usable work area (excluding taskbar and docked appbars).
pub fn get_desktop_work_area(window: Option<&Window>) -> WorkAreaRect {
    #[cfg(target_os = "windows")]
    {
        if let Some(win) = window {
            if let Ok(handle) = win.window_handle() {
                if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
                    let hwnd = HWND(win32_handle.hwnd.get() as *mut _);
                    let hmon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
                    if !hmon.is_invalid() {
                        let mut mi = MONITORINFO {
                            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                            ..Default::default()
                        };
                        if unsafe { GetMonitorInfoW(hmon, &mut mi).as_bool() } {
                            return WorkAreaRect {
                                left: mi.rcWork.left,
                                top: mi.rcWork.top,
                                right: mi.rcWork.right,
                                bottom: mi.rcWork.bottom,
                            };
                        }
                    }
                }
            }
        }

        // Global fallback via SPI_GETWORKAREA
        let mut rect = RECT::default();
        unsafe {
            let _ = SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut rect as *mut _ as *mut _),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            );
        }
        WorkAreaRect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        WorkAreaRect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        }
    }
}

/// Retrieves global cursor position in screen coordinates.
pub fn get_cursor_screen_pos() -> (i32, i32) {
    #[cfg(target_os = "windows")]
    {
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        (pt.x, pt.y)
    }

    #[cfg(not(target_os = "windows"))]
    {
        (0, 0)
    }
}
