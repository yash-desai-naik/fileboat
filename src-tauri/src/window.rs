//! Window size and place: every window opens fitted to its screen and centered, then the page
//! maximizes and shows it, so "Restore Down" returns to a sensible rectangle.

/// The "normal" (not maximized) window rectangle on a screen: 85% of its work area (the screen
/// minus the taskbar), centered, never below the minimum size. Physical pixels: (x, y, w, h).
fn fitted_rect(m: &tauri::Monitor) -> (i32, i32, i32, i32) {
    let area = m.work_area();
    let (aw, ah) = (area.size.width as f64, area.size.height as f64);
    let scale = m.scale_factor();
    let w = (aw * 0.85).max(720.0 * scale).min(aw).round();
    let h = (ah * 0.85).max(480.0 * scale).min(ah).round();
    let x = area.position.x as f64 + ((aw - w) / 2.0).round();
    let y = area.position.y as f64 + ((ah - h) / 2.0).round();
    (x as i32, y as i32, w as i32, h as i32)
}

fn monitor_of(window: &tauri::WebviewWindow, monitor: Option<tauri::Monitor>) -> Option<tauri::Monitor> {
    monitor
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())
}

/// Size a (still hidden) window to fit its screen, centered. The page then maximizes and shows
/// it (main.tsx) and calls `window_fit_restore`, so "Restore Down" returns to this rectangle.
pub fn place_centered(window: &tauri::WebviewWindow, monitor: Option<tauri::Monitor>) {
    if let Some(m) = monitor_of(window, monitor) {
        let (x, y, w, h) = fitted_rect(&m);
        let _ = window.set_size(tauri::PhysicalSize::new(w as u32, h as u32));
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

/// Make "Restore Down" return to the fitted, centered rectangle. A window maximized while still
/// hidden (to avoid a flash on start) never gets a proper restore position from Windows, so set
/// it directly: SetWindowPlacement changes only the restore rectangle, not the maximized state.
#[tauri::command]
pub fn window_fit_restore(window: tauri::WebviewWindow) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{GetWindowPlacement, SetWindowPlacement, WINDOWPLACEMENT};
        let Some(m) = monitor_of(&window, None) else { return };
        let Ok(hwnd) = window.hwnd() else { return };
        let (x, y, w, h) = fitted_rect(&m);
        // The restore rectangle is in "workspace" coordinates: screen coordinates shifted by the
        // primary screen's taskbar when that sits at the top or left.
        let (ox, oy) = window
            .primary_monitor()
            .ok()
            .flatten()
            .map(|p| (p.work_area().position.x - p.position().x, p.work_area().position.y - p.position().y))
            .unwrap_or((0, 0));
        let mut wp = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
        unsafe {
            if GetWindowPlacement(hwnd, &mut wp).is_ok() {
                wp.rcNormalPosition.left = x - ox;
                wp.rcNormalPosition.top = y - oy;
                wp.rcNormalPosition.right = x - ox + w;
                wp.rcNormalPosition.bottom = y - oy + h;
                let _ = SetWindowPlacement(hwnd, &wp);
            }
        }
    }
    #[cfg(not(windows))]
    let _ = window;
}
