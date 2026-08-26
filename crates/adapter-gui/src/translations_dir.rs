//! Responsibility: finds where the compiled translations live.

#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};

/// Search the filesystem for the gettext catalog directory containing
/// `<lang>/LC_MESSAGES/adapter-gui.mo`. Same path order as before:
///
/// 1. `OPENRIG_TRANSLATIONS_DIR` env var (developer override)
/// 2. `<exec_dir>/translations` (Windows next-to-exe, Mac.app/Resources)
/// 3. `<exec_dir>/../share/openrig/translations` (Linux FHS / .deb)
/// 4. `<exec_dir>/../Resources/translations` (Mac.app fallback)
/// 5. `CARGO_MANIFEST_DIR/translations` (debug builds running via `cargo run`)
///
/// Linux-only: only the libintl/gettext consumer needs this. Other platforms
/// rely on Slint bundled translations.
#[cfg(target_os = "linux")]
pub fn resolve_translations_dir() -> Option<PathBuf> {
    if let Ok(env_dir) = std::env::var("OPENRIG_TRANSLATIONS_DIR") {
        let p = PathBuf::from(env_dir);
        if has_any_mo(&p) {
            return Some(p);
        }
    }

    let exec = std::env::current_exe().ok()?;
    let exec_dir = exec.parent()?;

    let candidates = [
        exec_dir.join("translations"),
        exec_dir
            .join("..")
            .join("share")
            .join("openrig")
            .join("translations"),
        exec_dir.join("..").join("Resources").join("translations"),
    ];
    for c in &candidates {
        if has_any_mo(c) {
            return Some(c.clone());
        }
    }

    // CARGO_MANIFEST_DIR is the absolute path to the crate at compile time.
    // On the developer machine it's the source tree and the .mo files live
    // there next to the .po. On a user's machine after a deb/dmg install
    // that path doesn't exist — we check existence before trusting it, so
    // it's safe to attempt outside debug builds too.
    // Without this, `cargo run --release` (or any non-bundled run) would
    // skip every candidate and dgettext would fall back to the msgid,
    // surfacing as "BTN-NEW-PROJECT" leaking into the UI.
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("translations");
    if has_any_mo(&dev) {
        return Some(dev);
    }

    None
}

#[cfg(target_os = "linux")]
fn has_any_mo(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in rd.flatten() {
        let mo = entry
            .path()
            .join("LC_MESSAGES")
            .join(format!("{}.mo", TEXT_DOMAIN));
        if mo.exists() {
            return true;
        }
    }
    false
}
