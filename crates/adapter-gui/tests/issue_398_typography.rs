//! #398 — the approved mockup sets the UI in Barlow and keeps Bebas Neue for
//! display text only (titles, the chain name, big readouts). A non-Latin
//! locale keeps its own script face for both, so nothing renders as tofu.

use std::path::{Path, PathBuf};

fn ui_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui")
}

fn ui(rel: &str) -> String {
    let path = ui_dir().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every app `.slint` file: the vendored widget kit and the render harnesses
/// are not part of the app's look.
fn app_slint_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.is_dir() {
            if name != "modules" {
                app_slint_files(&path, out);
            }
        } else if name.ends_with(".slint") && !name.contains("harness") && !name.contains("mockup")
        {
            out.push(path);
        }
    }
}

#[test]
fn the_ui_face_is_barlow() {
    let locale = ui("locale_globals.slint");
    assert!(locale.contains("in-out property <string> font-family: \"Barlow\";"));
    let app = ui("app-window.slint");
    for weight in ["Regular", "Medium", "SemiBold", "Bold"] {
        let import = format!("import \"fonts/Barlow/Barlow-{weight}.ttf\";");
        assert!(app.contains(&import), "the app registers {import}");
        assert!(ui_dir()
            .join(format!("fonts/Barlow/Barlow-{weight}.ttf"))
            .is_file());
    }
    assert!(
        ui_dir().join("fonts/Barlow/OFL.txt").is_file(),
        "the font ships with its licence"
    );
}

#[test]
fn display_text_takes_the_locale_display_face() {
    let locale = ui("locale_globals.slint");
    assert!(
        locale.contains("out property <string> display-font-family:"),
        "titles read their face from the locale, so a CJK locale keeps its script face"
    );
    let mut files = Vec::new();
    app_slint_files(&ui_dir(), &mut files);
    for path in files {
        let src = std::fs::read_to_string(&path).unwrap();
        let rel = path.strip_prefix(ui_dir()).unwrap().display().to_string();
        if rel == "locale_globals.slint" {
            continue;
        }
        assert!(
            !src.contains("font-family: \"Bebas Neue\""),
            "{rel}: a display text uses Locale.display-font-family, never the face by name"
        );
    }
}

#[test]
fn every_tool_window_follows_the_locale_face() {
    for window in [
        "pages/tuner_window.slint",
        "pages/spectrum_window.slint",
        "pages/metronome_window.slint",
        "pages/mixer_window.slint",
        "pages/player_window.slint",
        "pages/drums_window.slint",
    ] {
        assert!(
            ui(window).contains("default-font-family: Locale.font-family;"),
            "{window}"
        );
    }
}

#[test]
fn the_mockup_titles_are_display_text() {
    for file in [
        "components/tool_bar.slint",
        "components/dialog_card.slint",
        "components/section_toggle.slint",
        "components/chain_title_preset.slint",
        "components/block_dialog_bar.slint",
        "components/block_panel_brand_zone.slint",
        "pages/project_chains.slint",
        "pages/settings.slint",
        "pages/tuner_window.slint",
        "pages/metronome_window.slint",
    ] {
        assert!(
            ui(file).contains("Locale.display-font-family"),
            "{file} sets its title in the display face"
        );
    }
}

#[test]
fn labels_are_upper_case_in_the_ui_face() {
    for file in [
        "components/field_caption.slint",
        "components/tag_pill.slint",
        "components/graph_node_card.slint",
    ] {
        assert!(
            ui(file).contains(".to-uppercase()"),
            "{file}: the mockup's labels are upper case; Barlow does not do it by itself"
        );
    }
}
