//! System tray icon management

use anyhow::Result;
use crossbeam_channel::Sender;
use log::{debug, info};
use rust_i18n::t;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

use crate::AppMessage;

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::*;
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::*;

/// Track if left mouse button is pressed on tray
static TRAY_MOUSE_DOWN: AtomicBool = AtomicBool::new(false);

/// Store initial mouse position for drag detection
static TRAY_DRAG_START: Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// Drag threshold in pixels
const DRAG_THRESHOLD: f64 = 5.0;

/// Shared state for window handle
pub static WINDOW_HWND: Mutex<Option<isize>> = Mutex::new(None);

/// Track window visibility
static WINDOW_VISIBLE: AtomicBool = AtomicBool::new(true);

/// Set the window handle for tray operations
pub fn set_window_hwnd(hwnd: isize) {
    *WINDOW_HWND.lock() = Some(hwnd);
}

/// Check if our window is currently the foreground (focused) window
#[cfg(windows)]
pub fn is_window_focused() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    if let Some(hwnd) = *WINDOW_HWND.lock() {
        unsafe {
            let foreground = GetForegroundWindow();
            foreground.0 as isize == hwnd
        }
    } else {
        false
    }
}

#[cfg(not(windows))]
pub fn is_window_focused() -> bool {
    false
}

/// Check if window is visible
pub fn is_window_visible() -> bool {
    WINDOW_VISIBLE.load(Ordering::SeqCst)
}

/// Hide the window
#[cfg(windows)]
pub fn hide_window() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

    if let Some(hwnd) = *WINDOW_HWND.lock() {
        unsafe {
            let hwnd = HWND(hwnd as *mut std::ffi::c_void);
            let _ = ShowWindow(hwnd, SW_HIDE);
            WINDOW_VISIBLE.store(false, Ordering::SeqCst);
            info!("Window hidden");
        }
    }
}

#[cfg(not(windows))]
pub fn hide_window() {
    // Not implemented for non-Windows
}

/// Move window to the monitor where the cursor is located
#[cfg(windows)]
fn move_window_to_cursor_monitor() {
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetWindowRect, SetWindowPos, HWND_TOP, SWP_NOSIZE, SWP_NOZORDER,
    };

    if let Some(hwnd) = *WINDOW_HWND.lock() {
        unsafe {
            let hwnd = HWND(hwnd as *mut std::ffi::c_void);

            // Get cursor position
            let mut cursor_pos = POINT::default();
            if GetCursorPos(&mut cursor_pos).is_err() {
                return;
            }

            // Get monitor at cursor position
            let monitor = MonitorFromPoint(cursor_pos, MONITOR_DEFAULTTONEAREST);

            // Get monitor info
            let mut monitor_info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut monitor_info).as_bool() {
                return;
            }

            // Get current window rect
            let mut window_rect = RECT::default();
            if GetWindowRect(hwnd, &mut window_rect).is_err() {
                return;
            }

            let window_width = window_rect.right - window_rect.left;
            let window_height = window_rect.bottom - window_rect.top;

            // Calculate centered position on the monitor
            let monitor_work = monitor_info.rcWork;
            let monitor_width = monitor_work.right - monitor_work.left;
            let monitor_height = monitor_work.bottom - monitor_work.top;

            let new_x = monitor_work.left + (monitor_width - window_width) / 2;
            let new_y = monitor_work.top + (monitor_height - window_height) / 2;

            // Move window to new position
            let _ = SetWindowPos(
                hwnd,
                HWND_TOP,
                new_x,
                new_y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER,
            );
            debug!(
                "Moved window to monitor at cursor position ({}, {})",
                new_x, new_y
            );
        }
    }
}

/// Show and activate the window using Windows API
#[cfg(windows)]
pub fn show_window() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
    };

    // First move window to cursor's monitor
    move_window_to_cursor_monitor();

    if let Some(hwnd) = *WINDOW_HWND.lock() {
        unsafe {
            let hwnd = HWND(hwnd as *mut std::ffi::c_void);
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            WINDOW_VISIBLE.store(true, Ordering::SeqCst);
            info!("Window shown and focused");
        }
    }
}

#[cfg(not(windows))]
pub fn show_window() {
    // Not implemented for non-Windows
}

/// Toggle window visibility - hide if focused, show if not
/// Returns true if window was shown, false if hidden
#[cfg(windows)]
pub fn toggle_window() -> bool {
    if is_window_focused() && is_window_visible() {
        info!("Window is focused, hiding");
        hide_window();
        false
    } else {
        info!("Window is not focused or hidden, showing");
        show_window();
        true
    }
}

#[cfg(not(windows))]
pub fn toggle_window() -> bool {
    show_window();
    true
}

/// Show a custom notification window near the system tray
#[cfg(windows)]
#[allow(dead_code)]
pub fn show_tray_notification(title: &str, message: &str) {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetTimer, GetMessageW, TranslateMessage, DispatchMessageW, MSG,
    };
    use windows::Win32::UI::HiDpi::GetDpiForSystem;

    let title = title.to_string();
    let message = message.to_string();

    std::thread::spawn(move || {

        unsafe {
            let class_name = w!("SukushoNotificationClass");

            // Register window class
            let hinstance = GetModuleHandleW(None).unwrap();

            let wc = WNDCLASSW {
                lpfnWndProc: Some(notification_wndproc),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                hbrBackground: CreateSolidBrush(COLORREF(0x00000000)), // Transparent black
                ..Default::default()
            };

            RegisterClassW(&wc);

            // Get taskbar position to position notification
            let mut taskbar_rect = RECT::default();
            if let Ok(taskbar) = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) {
                let _ = GetWindowRect(taskbar, &mut taskbar_rect);
            }

            // Get DPI for proper scaling
            let dpi = GetDpiForSystem();
            let scale = dpi as f32 / 96.0; // 96 is the standard DPI

            // Get screen dimensions
            let screen_width = GetSystemMetrics(SM_CXSCREEN);
            let screen_height = GetSystemMetrics(SM_CYSCREEN);

            // Notification dimensions (scaled for DPI)
            let notif_width = (400.0 * scale) as i32;
            let notif_height = (140.0 * scale) as i32;
            let margin = (15.0 * scale) as i32;

            // Position in bottom-right corner, above taskbar
            let x = screen_width - notif_width - margin;
            let y = if taskbar_rect.bottom > taskbar_rect.top {
                taskbar_rect.top - notif_height - margin
            } else {
                screen_height - notif_height - margin
            };

            // Create layered window
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class_name,
                w!("Sukusho Notification"),
                WS_POPUP,
                x,
                y,
                notif_width,
                notif_height,
                None,
                None,
                hinstance,
                None,
            ).unwrap();

            // Set layered window attributes for transparency
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 230, LWA_ALPHA);

            // Show window with fade-in animation
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = AnimateWindow(hwnd, 200, AW_BLEND);

            // Store title, message, and scale in window data
            let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
            let message_wide: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(Box::new((title_wide, message_wide, scale))) as isize);

            // Invalidate to trigger paint
            let _ = InvalidateRect(hwnd, None, true);
            let _ = UpdateWindow(hwnd);

            // Set a timer to auto-close after 5 seconds (timer ID = 1)
            let _ = SetTimer(hwnd, 1, 5000, None);

            // Message loop to keep window alive
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }
        }
    });
}

#[cfg(windows)]
#[allow(dead_code)]
unsafe extern "system" fn notification_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT { unsafe {
    use windows::core::w;
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
    use windows::Win32::UI::WindowsAndMessaging::KillTimer;

    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            // Get stored title, message, and scale
            let user_data = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if user_data != 0 {
                let data_ptr = user_data as *const (Vec<u16>, Vec<u16>, f32);
                let (title, message, scale) = &*data_ptr;

                // Set up drawing
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);

                // Fill background with semi-transparent dark color
                let bg_brush = CreateSolidBrush(COLORREF(0x00262626)); // Dark gray
                let _ = FillRect(hdc, &rect, bg_brush);
                let _ = DeleteObject(bg_brush);

                // Draw border
                let border_brush = CreateSolidBrush(COLORREF(0x00404040)); // Light gray border
                let _ = FrameRect(hdc, &rect, border_brush);
                let _ = DeleteObject(border_brush);

                // Set text properties
                let _ = SetBkMode(hdc, TRANSPARENT);
                let _ = SetTextColor(hdc, COLORREF(0x00FFFFFF)); // White text

                // Calculate scaled dimensions
                let padding = (20.0 * scale) as i32;
                let title_height = (35.0 * scale) as i32;
                let message_top = (60.0 * scale) as i32;
                let message_bottom_margin = (15.0 * scale) as i32;

                // Draw title (bold, DPI-scaled font)
                let mut title_rect = rect.clone();
                title_rect.top += padding;
                title_rect.left += padding;
                title_rect.right -= padding;
                title_rect.bottom = title_rect.top + title_height;

                let title_font = CreateFontW(
                    (20.0 * scale) as i32, 0, 0, 0, 700, 0, 0, 0, // DPI-scaled, FW_BOLD
                    1, // DEFAULT_CHARSET
                    0, // OUT_DEFAULT_PRECIS
                    0, // CLIP_DEFAULT_PRECIS
                    5, // CLEARTYPE_QUALITY
                    0, // DEFAULT_PITCH | FF_DONTCARE
                    w!("Segoe UI"),
                );
                let old_font = SelectObject(hdc, title_font);
                let mut title_slice = title.clone();
                let _ = DrawTextW(hdc, &mut title_slice[..], &mut title_rect, DT_LEFT | DT_TOP | DT_SINGLELINE);
                let _ = SelectObject(hdc, old_font);
                let _ = DeleteObject(title_font);

                // Draw message (DPI-scaled font)
                let mut message_rect = rect.clone();
                message_rect.top += message_top;
                message_rect.left += padding;
                message_rect.right -= padding;
                message_rect.bottom -= message_bottom_margin;

                let message_font = CreateFontW(
                    (16.0 * scale) as i32, 0, 0, 0, 400, 0, 0, 0, // DPI-scaled, FW_NORMAL
                    1, // DEFAULT_CHARSET
                    0, // OUT_DEFAULT_PRECIS
                    0, // CLIP_DEFAULT_PRECIS
                    5, // CLEARTYPE_QUALITY
                    0, // DEFAULT_PITCH | FF_DONTCARE
                    w!("Segoe UI"),
                );
                let old_font = SelectObject(hdc, message_font);
                let mut message_slice = message.clone();
                let _ = DrawTextW(hdc, &mut message_slice[..], &mut message_rect, DT_LEFT | DT_TOP | DT_WORDBREAK);
                let _ = SelectObject(hdc, old_font);
                let _ = DeleteObject(message_font);
            }

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            // Clean up user data
            let user_data = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if user_data != 0 {
                let data_ptr = user_data as *mut (Vec<u16>, Vec<u16>, f32);
                drop(Box::from_raw(data_ptr));
            }
            LRESULT(0)
        }
        WM_TIMER => {
            // Timer fired - close the window with animation
            let _ = KillTimer(hwnd, 1);
            let _ = AnimateWindow(hwnd, 300, AW_HIDE | AW_BLEND);
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            // Close on click with faster animation
            let _ = KillTimer(hwnd, 1);
            let _ = AnimateWindow(hwnd, 150, AW_HIDE | AW_BLEND);
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}}

#[cfg(not(windows))]
pub fn show_tray_notification(_title: &str, _message: &str) {
    // Not implemented for non-Windows
}

pub struct TrayManager {
    _tray_icon: TrayIcon,
}

impl TrayManager {
    pub fn new(message_tx: Sender<AppMessage>) -> Result<Self> {
        info!("Creating tray icon...");

        let menu = Menu::new();
        let settings_item = MenuItem::new(&t!("tray.menu.settings"), true, None);
        let check_updates_item = MenuItem::new(&t!("tray.menu.check_for_updates"), true, None);
        let quit_item = MenuItem::new(&t!("tray.menu.quit"), true, None);

        menu.append_items(&[
            &settings_item,
            &check_updates_item,
            &PredefinedMenuItem::separator(),
            &quit_item,
        ])?;

        let icon = Self::generate_camera_icon()?;

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(&t!("tray.tooltip"))
            .with_icon(icon)
            .with_menu_on_left_click(false)
            .build()?;

        let menu_tx = message_tx.clone();
        let settings_id = settings_item.id().clone();
        let check_updates_id = check_updates_item.id().clone();
        let quit_id = quit_item.id().clone();

        std::thread::spawn(move || {
            let menu_receiver = MenuEvent::receiver();
            loop {
                if let Ok(event) = menu_receiver.recv() {
                    if event.id == settings_id {
                        show_window();
                        let _ = menu_tx.send(AppMessage::OpenSettings);
                    } else if event.id == check_updates_id {
                        info!("Check for updates requested from tray menu");
                        std::thread::spawn(|| {
                            use crate::update_checker;
                            info!("{}", rust_i18n::t!("notifications.update.checking"));

                            match update_checker::check_for_updates() {
                                Ok(has_update) => {
                                    if has_update {
                                        info!("{}", rust_i18n::t!("notifications.update.available"));
                                        update_checker::open_releases_page();
                                    } else {
                                        info!("{}", rust_i18n::t!("notifications.update.up_to_date"));
                                    }
                                }
                                Err(e) => {
                                    log::warn!("Failed to check for updates: {}", e);
                                    log::warn!("{}", rust_i18n::t!("notifications.update.check_failed"));
                                }
                            }
                        });
                    } else if event.id == quit_id {
                        info!("Quit requested from tray menu");
                        std::process::exit(0);
                    }
                }
            }
        });

        let click_tx = message_tx.clone();
        std::thread::spawn(move || {
            let tray_receiver = TrayIconEvent::receiver();
            loop {
                if let Ok(event) = tray_receiver.recv() {
                    match event {
                        TrayIconEvent::Click {
                            button: tray_icon::MouseButton::Left,
                            button_state: tray_icon::MouseButtonState::Down,
                            position,
                            ..
                        } => {
                            *TRAY_DRAG_START.lock() = Some((position.x, position.y));
                            TRAY_MOUSE_DOWN.store(true, Ordering::SeqCst);
                        }
                        TrayIconEvent::Click {
                            button: tray_icon::MouseButton::Left,
                            button_state: tray_icon::MouseButtonState::Up,
                            ..
                        } => {
                            if TRAY_MOUSE_DOWN.load(Ordering::SeqCst) {
                                TRAY_MOUSE_DOWN.store(false, Ordering::SeqCst);
                                *TRAY_DRAG_START.lock() = None;
                                let was_shown = toggle_window();
                                // If window was shown, send message to reset to main view
                                if was_shown {
                                    let _ = click_tx.send(AppMessage::ShowMainWindow);
                                }
                            }
                        }
                        TrayIconEvent::Move { position, .. } => {
                            if TRAY_MOUSE_DOWN.load(Ordering::SeqCst) {
                                if let Some((start_x, start_y)) = *TRAY_DRAG_START.lock() {
                                    let dx = position.x - start_x;
                                    let dy = position.y - start_y;
                                    let distance = (dx * dx + dy * dy).sqrt();

                                    if distance > DRAG_THRESHOLD {
                                        TRAY_MOUSE_DOWN.store(false, Ordering::SeqCst);
                                        *TRAY_DRAG_START.lock() = None;

                                        if let Some(latest_path) = crate::get_latest_screenshot() {
                                            info!("Starting tray drag with: {:?}", latest_path);
                                            crate::drag_drop::start_drag(&[latest_path]);
                                        } else {
                                            debug!("No screenshots available for tray drag");
                                        }
                                    }
                                }
                            }
                        }
                        TrayIconEvent::Leave { .. } => {
                            if TRAY_MOUSE_DOWN.load(Ordering::SeqCst) {
                                TRAY_MOUSE_DOWN.store(false, Ordering::SeqCst);
                                *TRAY_DRAG_START.lock() = None;

                                if let Some(latest_path) = crate::get_latest_screenshot() {
                                    info!("Starting tray drag (leave) with: {:?}", latest_path);
                                    crate::drag_drop::start_drag(&[latest_path]);
                                }
                            }
                        }
                        TrayIconEvent::DoubleClick {
                            button: tray_icon::MouseButton::Left,
                            ..
                        } => {
                            TRAY_MOUSE_DOWN.store(false, Ordering::SeqCst);
                            *TRAY_DRAG_START.lock() = None;
                            show_window();
                            let _ = click_tx.send(AppMessage::ToggleWindow);
                        }
                        _ => {}
                    }
                }
            }
        });

        info!("Tray icon created successfully");
        Ok(Self {
            _tray_icon: tray_icon,
        })
    }

    fn generate_camera_icon() -> Result<Icon> {
        let size = 32u32;
        let mut rgba = vec![0u8; (size * size * 4) as usize];

        for y in 0..size {
            for x in 0..size {
                let idx = ((y * size + x) * 4) as usize;
                let fx = x as f32 / size as f32;
                let fy = y as f32 / size as f32;

                let in_body = fx > 0.1 && fx < 0.9 && fy > 0.25 && fy < 0.85;
                let cx = 0.5;
                let cy = 0.55;
                let r = 0.22;
                let dist = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                let in_lens = dist < r;
                let in_lens_inner = dist < r * 0.6;
                let in_flash = fx > 0.6 && fx < 0.8 && fy > 0.12 && fy < 0.28;

                if in_lens_inner {
                    rgba[idx] = 100;
                    rgba[idx + 1] = 180;
                    rgba[idx + 2] = 255;
                    rgba[idx + 3] = 255;
                } else if in_lens {
                    rgba[idx] = 40;
                    rgba[idx + 1] = 40;
                    rgba[idx + 2] = 50;
                    rgba[idx + 3] = 255;
                } else if in_body || in_flash {
                    rgba[idx] = 60;
                    rgba[idx + 1] = 60;
                    rgba[idx + 2] = 70;
                    rgba[idx + 3] = 255;
                } else {
                    rgba[idx] = 0;
                    rgba[idx + 1] = 0;
                    rgba[idx + 2] = 0;
                    rgba[idx + 3] = 0;
                }
            }
        }

        Icon::from_rgba(rgba, size, size)
            .map_err(|e| anyhow::anyhow!("Failed to create generated icon: {}", e))
    }

    /// Update tray icon tooltip
    pub fn update_tooltip(&mut self, text: &str) {
        if let Err(e) = self._tray_icon.set_tooltip(Some(text)) {
            log::warn!("Failed to update tray tooltip: {}", e);
        }
    }
}
