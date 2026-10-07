//! A window built at startup and shown later from a callback (tuner, drums,
//! backing tracks, ...) must paint its first frame. The winit backend before
//! Slint 1.18 rendered that pre-show frame only for a window shown right after
//! its creation, so on macOS such a window opened as a bare title bar or a
//! blank pane until something inside it changed (upstream fix
//! slint-ui/slint#13322, released in 1.18.0). The lock file must keep a winit
//! backend that carries the fix.

use std::path::Path;

fn locked_version(lock: &str, crate_name: &str) -> (u32, u32, u32) {
    let entry = format!("name = \"{crate_name}\"\nversion = \"");
    let at = lock
        .find(&entry)
        .unwrap_or_else(|| panic!("{crate_name} is in Cargo.lock"))
        + entry.len();
    let version = &lock[at..at + lock[at..].find('"').unwrap()];
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap());
    (
        parts.next().unwrap(),
        parts.next().unwrap(),
        parts.next().unwrap(),
    )
}

#[test]
fn the_winit_backend_paints_a_window_shown_after_startup() {
    let lock =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"))
            .unwrap();
    let version = locked_version(&lock, "i-slint-backend-winit");
    assert!(
        version >= (1, 18, 0),
        "i-slint-backend-winit {version:?} maps a later-shown window without its first frame"
    );
}
