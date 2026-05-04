use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // with_bundled_translations embeds the .po files directly into the
    // compiled binary at compile time. This sidesteps every runtime path
    // resolution issue: no bindtextdomain, no env-var dependency, no
    // POSIX-vs-BCP47 dir name confusion, and no IDE/launcher quirks
    // (RustRover, .app double-click, cargo run from a tmp cwd, etc.).
    //
    // The locale Slint translate() picks at runtime comes from
    // `slint::select_bundled_translation(...)` which we call from
    // i18n::init_translations once we resolve the user's preference.
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let translations = manifest.join("translations");
    let config = if translations.exists() {
        slint_build::CompilerConfiguration::new()
            .with_bundled_translations(translations)
    } else {
        slint_build::CompilerConfiguration::new()
    };
    slint_build::compile_with_config("ui/app-window.slint", config)
        .expect("failed to compile Slint UI");

    // Keep msgfmt-driven .mo generation as a build artifact for packaging
    // scripts (macOS .app bundle, .deb, Windows installer) — even though
    // the runtime no longer needs them, the packaging pipeline does for
    // the staged distribution layout.
    compile_translations();
}

/// Compile every `translations/<lang>/adapter-gui.po` into runtime-loadable
/// `.mo` files. Output goes to two places:
///   - `<OUT_DIR>/translations/<lang>/LC_MESSAGES/adapter-gui.mo` (cargo run dev workflow)
///   - `translations/<lang>/LC_MESSAGES/adapter-gui.mo` (in-source, gitignored)
///     so packaging scripts have a stable path.
///
/// The text-domain name follows Slint's default (`CARGO_PKG_NAME` →
/// `adapter-gui`). End users never see this identifier — it's just the gettext
/// catalog name.
///
/// If `msgfmt` is missing (typical on Windows / fresh macOS), this step is
/// skipped silently with a single one-line note; the runtime falls back to
/// the bundled translations Slint already embedded via
/// `with_bundled_translations`, so the app stays fully functional.
fn compile_translations() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let translations_src = manifest.join("translations");
    if !translations_src.exists() {
        return;
    }
    println!("cargo:rerun-if-changed=translations");

    if !msgfmt_available() {
        println!(
            "cargo:warning=msgfmt not on PATH — skipping .mo generation \
             (runtime uses Slint bundled translations; install GNU gettext \
             only if you're packaging .deb / Linux distros that need .mo files)"
        );
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));
    let out_translations = out_dir.join("translations");

    let langs = match std::fs::read_dir(&translations_src) {
        Ok(rd) => rd,
        Err(e) => {
            println!("cargo:warning=cannot read translations dir: {}", e);
            return;
        }
    };
    for entry in langs.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(lang) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let po = path.join("LC_MESSAGES").join("adapter-gui.po");
        if !po.exists() {
            continue;
        }
        println!("cargo:rerun-if-changed={}", po.display());

        compile_po(&po, &out_translations.join(lang).join("LC_MESSAGES"), lang);
        compile_po(&po, &path.join("LC_MESSAGES"), lang);
    }
}

/// Returns true iff `msgfmt --version` runs successfully. Cached for the
/// lifetime of the build script (cheap — at most one probe per build).
fn msgfmt_available() -> bool {
    Command::new("msgfmt")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn compile_po(po: &Path, mo_dir: &Path, lang: &str) {
    if let Err(e) = std::fs::create_dir_all(mo_dir) {
        println!("cargo:warning=cannot create {} ({})", mo_dir.display(), e);
        return;
    }
    let mo = mo_dir.join("adapter-gui.mo");
    let status = Command::new("msgfmt")
        .arg("-o")
        .arg(&mo)
        .arg(po)
        .status();
    if let Ok(s) = status {
        if !s.success() {
            // msgfmt exists but rejected this .po file — that IS a real bug
            // (malformed catalog). Surface it so the translator can fix it.
            println!(
                "cargo:warning=msgfmt rejected {}/adapter-gui.po (exit {}) — fix the .po file",
                lang,
                s.code().unwrap_or(-1)
            );
        }
    }
    // Err case is unreachable here because msgfmt_available() gates entry.
}
