use crate::AppWindow;
use slint::{ComponentHandle, Global, Timer, TimerMode};
use std::fmt::Display;
use std::time::Duration;

pub(crate) fn log_gui_message(context: &str, message: &str) {
    log::info!("[adapter-gui] {context}: {message}");
}

pub(crate) fn log_gui_error(context: &str, error: impl Display) {
    log::error!("[adapter-gui] {context}: {error}");
}

pub(crate) fn show_child_window(parent_window: &slint::Window, child_window: &slint::Window) {
    let pos = parent_window.position();
    log::warn!("[UI] show_child_window: parent_pos=({},{})", pos.x, pos.y);
    child_window.set_position(slint::WindowPosition::Physical(slint::PhysicalPosition {
        x: pos.x + 40,
        y: pos.y + 40,
    }));
    match child_window.show() {
        Ok(_) => log::warn!("[UI] show_child_window: success"),
        Err(e) => log::error!("[UI] show_child_window: FAILED: {e}"),
    }
}

pub(crate) fn use_inline_block_editor(window: &AppWindow) -> bool {
    window.get_fullscreen()
        || (window.get_touch_optimized()
            && window
                .get_interaction_mode_label()
                .to_string()
                .to_lowercase()
                .contains("touch"))
}

/// Sets a toast notification on the main window and starts the auto-dismiss timer.
/// Also sets `status_message` for backward compatibility with pages that still reference it.
pub(crate) fn set_status_with_toast(
    window: &AppWindow,
    toast_timer: &Timer,
    message: &str,
    level: &str,
) {
    window.set_status_message(message.into());
    crate::OverlayBridge::get(window).set_toast_message(message.into());
    crate::OverlayBridge::get(window).set_toast_level(level.into());
    if !message.is_empty() {
        match level {
            "error" => {
                log::error!("{}", message);
                eprintln!("[ERROR] {}", message);
            }
            "warning" => {
                log::warn!("{}", message);
                eprintln!("[WARN] {}", message);
            }
            _ => {
                log::info!("{}", message);
                eprintln!("[INFO] {}", message);
            }
        }
        let weak = window.as_weak();
        toast_timer.start(TimerMode::SingleShot, Duration::from_secs(3), move || {
            if let Some(window) = weak.upgrade() {
                crate::OverlayBridge::get(&window).set_toast_message("".into());
                crate::OverlayBridge::get(&window).set_toast_level("info".into());
                window.set_status_message("".into());
            }
        });
    }
}

pub(crate) fn set_status_error(window: &AppWindow, toast_timer: &Timer, message: &str) {
    set_status_with_toast(window, toast_timer, message, "error");
}

pub(crate) fn set_status_info(window: &AppWindow, toast_timer: &Timer, message: &str) {
    set_status_with_toast(window, toast_timer, message, "info");
}

pub(crate) fn set_status_warning(window: &AppWindow, toast_timer: &Timer, message: &str) {
    set_status_with_toast(window, toast_timer, message, "warning");
}

pub(crate) fn clear_status(window: &AppWindow, toast_timer: &Timer) {
    toast_timer.stop();
    window.set_status_message("".into());
    crate::OverlayBridge::get(window).set_toast_message("".into());
    crate::OverlayBridge::get(window).set_toast_level("info".into());
}

/// Best-effort BCP-47-ish locale tag from `LANG` (`pt_BR.UTF-8` → `pt-BR`),
/// defaulting to `en-US` when the env is missing, POSIX or empty. Used where
/// a plugin/block description is shown in the user's language.
pub(crate) fn system_language() -> String {
    let lang = std::env::var("LANG").unwrap_or_default();
    let base = lang.split('.').next().unwrap_or("");
    // "C", "POSIX", empty, or too short = not a real locale → fall back to English
    if base.is_empty() || base.len() < 2 || matches!(base, "C" | "POSIX") {
        return "en-US".to_string();
    }
    base.replace('_', "-")
}
