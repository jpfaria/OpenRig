//! #398 — the light/dark scheme is a setting: Settings → Appearance offers
//! System / Light / Dark, and every window paints in the chosen one.

use std::path::PathBuf;

fn read(dir: &str, rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(dir)
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn ui(rel: &str) -> String {
    read("ui", rel)
}

fn src(rel: &str) -> String {
    read("src", rel)
}

#[test]
fn the_theme_takes_the_chosen_mode_before_the_system_scheme() {
    let theme = ui("theme.slint");
    assert!(theme.contains("export enum ThemeMode { system, light, dark }"));
    assert!(theme.contains("in property <ThemeMode> mode: ThemeMode.system;"));
    assert!(theme.contains(
        "out property <bool> dark: root.mode == ThemeMode.dark\n        || (root.mode == ThemeMode.system && SystemPalette.color-scheme != ColorScheme.light);"
    ));
}

#[test]
fn rust_reaches_the_theme_of_every_window() {
    assert!(ui("app-window.slint").contains("export { Theme, ThemeMode } from \"theme.slint\";"));
}

#[test]
fn settings_lists_an_appearance_section() {
    let page = ui("pages/settings.slint");
    assert_eq!(
        page.matches("@tr(\"title-section-appearance\")").count(),
        2,
        "nav row + heading"
    );
    assert!(page.contains("if root.selected-section == 7 : SectionSystemAppearance {}"));
    assert!(page.contains("@image-url(\"../assets/nav-appearance.svg\")"));
    let section = ui("pages/settings/section_system_appearance.slint");
    assert!(section.starts_with("// Responsibility: "));
    assert!(section.contains("SegControl {"));
    assert!(section.contains("current: SettingsBridge.appearance-index;"));
    assert!(section.contains("picked(i) => { SettingsBridge.appearance-selected(i); }"));
    for key in [
        "appearance-system",
        "appearance-light",
        "appearance-dark",
        "label-theme",
    ] {
        assert!(section.contains(&format!("@tr(\"{key}\")")), "{key}");
    }
    let bridge = ui("settings_globals.slint");
    assert!(bridge.contains("in-out property <int> appearance-index;"));
    assert!(bridge.contains("callback appearance-selected(int);"));
}

#[test]
fn every_window_open_at_boot_gets_the_scheme() {
    let boot = src("desktop_app_windows.rs");
    let fan_out = src("desktop_app_appearance.rs");
    let windows: Vec<&str> = boot
        .lines()
        .filter_map(|l| {
            l.split("Window::new()")
                .next()
                .filter(|_| l.contains("Window::new()"))
        })
        .filter_map(|head| head.rsplit(|c: char| !c.is_alphanumeric()).next())
        .map(|name| name.trim())
        .collect();
    assert!(windows.len() >= 10, "boot windows: {windows:?}");
    for window in windows {
        let ty = format!("{window}Window");
        assert!(fan_out.contains(&ty), "the scheme never reaches {ty}");
    }
}

#[test]
fn a_window_opened_later_starts_in_the_chosen_scheme() {
    for file in [
        "chain_crud_wiring.rs",
        "block_editor_window_setup.rs",
        "compact_chain_callbacks.rs",
        "block_editor_window_delete.rs",
    ] {
        let body = src(file);
        let opened = body.matches("Window::new()").count();
        let themed = body
            .matches("crate::settings::appearance_followers::follow(&")
            .count();
        assert!(opened > 0, "{file} opens no window");
        assert_eq!(
            opened, themed,
            "{file}: every window it opens takes the chosen scheme and keeps following it"
        );
    }
}

#[test]
fn a_scheme_chosen_over_mcp_repaints_the_app() {
    assert!(
        src("chain_rig_nav_wiring.rs").contains("crate::appearance_events::apply(events)"),
        "the MCP/MIDI drain hands the scheme to the windows"
    );
    assert!(
        src("settings/appearance.rs").contains("crate::appearance_events::install("),
        "the Settings pick and an MCP pick repaint through the same path"
    );
    assert!(
        src("desktop_app_appearance.rs").contains("appearance_followers::repaint_all(appearance)"),
        "a pick repaints the windows opened on demand too"
    );
}
