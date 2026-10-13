//! The panel window: showing, hiding, and the small window plumbing that makes
//! it behave like a menu bar popover rather than a normal window.

use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Rect, WebviewWindow};
pub const LABEL: &str = "panel";
pub const WIDTH: f64 = 344.0;
const MIN_HEIGHT: f64 = 200.0;
const MAX_HEIGHT: f64 = 760.0;
/// Gap between the menu bar and the top of the panel.
const GAP: f64 = 6.0;
/// A click that lands right after a blur-hide must not immediately reopen.
const REOPEN_GRACE: Duration = Duration::from_millis(280);

static LAST_AUTO_HIDE: Mutex<Option<Instant>> = Mutex::new(None);

pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

/// Called when the panel hid itself because it lost focus.
pub fn note_auto_hide() {
    if let Ok(mut slot) = LAST_AUTO_HIDE.lock() {
        *slot = Some(Instant::now());
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(win) = window(app) {
        let _ = win.hide();
    }
}

/// Tray click: close when open, open when closed.
pub fn toggle(app: &AppHandle, rect: Option<Rect>) {
    let Some(win) = window(app) else { return };
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
        return;
    }
    if let Ok(slot) = LAST_AUTO_HIDE.lock() {
        if let Some(at) = *slot {
            if at.elapsed() < REOPEN_GRACE {
                return;
            }
        }
    }
    show(app, rect);
}

pub fn show(app: &AppHandle, rect: Option<Rect>) {
    let Some(win) = window(app) else { return };
    match rect {
        Some(rect) => place_under(&win, &rect),
        None => place_default(&win),
    }
    let _ = win.show();
    activate();
    let _ = win.set_focus();
}

/// Keep the native window exactly as tall as the rendered view.
pub fn resize(app: &AppHandle, height: f64) {
    let Some(win) = window(app) else { return };
    let height = height.clamp(MIN_HEIGHT, MAX_HEIGHT);
    let scale = win.scale_factor().unwrap_or(2.0);
    if let Ok(size) = win.inner_size() {
        let logical = size.to_logical::<f64>(scale);
        if (logical.height - height).abs() < 1.0 {
            return;
        }
    }
    let _ = win.set_size(LogicalSize::new(WIDTH, height));
}

/// Centre the panel on the tray icon and hang it just below the menu bar.
fn place_under(win: &WebviewWindow, rect: &Rect) {
    let scale = win.scale_factor().unwrap_or(2.0);
    let icon = rect.position.to_logical::<f64>(scale);
    let icon_size = rect.size.to_logical::<f64>(scale);
    let panel = panel_size(win, scale);

    let mut x = icon.x + icon_size.width / 2.0 - panel / 2.0;
    let y = icon.y + icon_size.height + GAP;

    if let Some(bounds) = monitor_containing(win, icon.x, icon.y) {
        x = x.clamp(bounds.0 + 6.0, bounds.0 + bounds.2 - WIDTH - 6.0);
    }
    let _ = win.set_position(LogicalPosition::new(x, y));
}

/// First run: no tray click yet, so park it under the right edge of the menu bar.
fn place_default(win: &WebviewWindow) {
    let Ok(Some(monitor)) = win.primary_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    let origin = monitor.position().to_logical::<f64>(scale);
    let size = monitor.size().to_logical::<f64>(scale);
    let x = origin.x + size.width - WIDTH - 12.0;
    let y = origin.y + 30.0;
    let _ = win.set_position(LogicalPosition::new(x, y));
}

fn panel_size(win: &WebviewWindow, scale: f64) -> f64 {
    win.outer_size()
        .map(|s| s.to_logical::<f64>(scale).width)
        .unwrap_or(WIDTH)
}

/// (x, y, width, height) of the monitor holding the point, in logical points.
fn monitor_containing(win: &WebviewWindow, x: f64, y: f64) -> Option<(f64, f64, f64, f64)> {
    let monitors = win.available_monitors().ok()?;
    for monitor in monitors {
        let scale = monitor.scale_factor();
        let origin = monitor.position().to_logical::<f64>(scale);
        let size = monitor.size().to_logical::<f64>(scale);
        if x >= origin.x && x <= origin.x + size.width && y >= origin.y && y <= origin.y + size.height
        {
            return Some((origin.x, origin.y, size.width, size.height));
        }
    }
    None
}

/// Accessory apps do not activate on their own, and a panel that never becomes
/// key would close again on the first blur. AppKit handles that on the main thread.
fn activate() {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
        }
    }
}
