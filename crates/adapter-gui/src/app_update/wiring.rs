//! Responsibility: wires the launcher's version label to the background update check.

use slint::{ComponentHandle, Global};

use super::fetch::fetch_latest_release_json;
use super::installer::launch_installer;
use super::offer::offered_update;
use super::running_version::current_version;
use crate::{AppUpdate, AppWindow};

/// The offer only exists on macOS, the one platform with a one-line
/// installer; elsewhere the version stays a plain label and nothing is
/// fetched. The GitHub request runs on its own thread so a slow or absent
/// network never delays the window.
pub(crate) fn wire_app_update(window: &AppWindow) {
    if !cfg!(target_os = "macos") {
        return;
    }
    AppUpdate::get(window).on_install_update(launch_installer);

    let current = current_version(
        std::env::var("OPENRIG_UPDATE_CURRENT_VERSION").ok(),
        env!("CARGO_PKG_VERSION"),
    );
    let weak = window.as_weak();
    let spawned = std::thread::Builder::new()
        .name("openrig-update-check".into())
        .spawn(move || {
            let Some(latest) = offered_update(fetch_latest_release_json().as_deref(), &current)
            else {
                return;
            };
            log::info!("update available: v{latest} (running v{current})");
            let _ = weak.upgrade_in_event_loop(move |w| {
                AppUpdate::get(&w).set_latest_version(latest.into());
            });
        });
    if let Err(e) = spawned {
        log::warn!("could not start the update check: {e}");
    }
}
