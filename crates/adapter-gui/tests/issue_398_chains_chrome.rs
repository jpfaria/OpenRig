//! #398 — the chains screen: a dark top bar edge to edge, chains stacked flush
//! under it, each with a light header (a pill switch, the instrument, the name
//! in the display face, the preset and scenes as fields, quiet icons).

use std::path::PathBuf;

fn ui(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn repo_asset(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_top_bar_is_dark_chrome_edge_to_edge() {
    let page = ui("pages/project_chains.slint");
    assert!(page.contains("background: Theme.chrome;"));
    assert!(page.contains("Theme.chrome-line"));
    assert!(page.contains("font-family: Locale.display-font-family;"));
    assert!(page.contains("on-chrome: true;"));
    assert!(
        !page.contains("x: 24px;"),
        "the bar and the chains run edge to edge"
    );
    assert!(
        !page.contains("row.height + 12px"),
        "chains stack flush, one hairline apart"
    );
    let chips = ui("components/chain_chips.slint");
    assert!(chips.contains("in property <bool> on-chrome"));
    assert!(chips.contains("Theme.chrome-fg-2"));
    assert!(chips.contains("Theme.chrome-2"));
}

#[test]
fn the_tempo_reads_on_the_chrome() {
    let tempo = ui("components/project_tempo.slint");
    assert!(tempo.contains("font-family: Locale.display-font-family;"));
    assert!(tempo.contains("color: Theme.chrome-fg;"));
    assert!(tempo.contains("on-chrome: true;"));
    assert!(tempo.contains("Theme.chrome-ring"));
}

#[test]
fn the_chain_header_is_a_light_bar_with_a_pill_switch() {
    let header = ui("pages/chain_row_header.slint");
    assert!(header.contains("background: Theme.panel-hi;"));
    assert!(header.contains("ToggleSwitch {"));
    assert!(!header.contains("PowerSwitch {"));
    assert!(
        !header.contains("tooltip: \""),
        "every tooltip is translated"
    );
    assert!(header.contains("@tr(\"tooltip-move-chain-up\")"));
    assert!(header.contains("@tr(\"tooltip-move-chain-down\")"));
    let switch = ui("components/toggle_switch.slint");
    assert!(switch.starts_with("// Responsibility: "));
    assert!(switch.contains("export component ToggleSwitch"));
    assert!(switch.contains("accessible-role: switch;"));
    assert!(switch.contains("Theme.accent"));
}

#[test]
fn the_header_icons_share_one_quiet_look() {
    let style = ui("components/header_icon_style.slint");
    assert!(style.starts_with("// Responsibility: "));
    assert!(style.contains("export global HeaderIconStyle"));
    assert!(style.contains("Theme.ink-3"));
    for button in [
        "components/chain_meters_button.slint",
        "components/chain_mixer_button.slint",
        "components/chain_looper_button.slint",
        "components/chain_tone_doctor_button.slint",
        "components/chain_di_loop_button.slint",
        "components/chain_volume_button.slint",
        "components/chain_chips.slint",
        "components/chain_title_preset.slint",
    ] {
        assert!(
            ui(button).contains("HeaderIconStyle."),
            "{button} draws its icon the shared way"
        );
    }
}

#[test]
fn the_name_preset_and_scenes_follow_the_mockup() {
    let title = ui("components/chain_title_preset.slint");
    assert!(title.contains("font-family: Locale.display-font-family;"));
    assert!(
        title.contains("background: root.active ? Theme.accent : root.on-chrome ? Theme.chrome-field : Theme.field;"),
        "a scene is a field, the active one filled with the accent"
    );
    let select = ui("components/preset_select.slint");
    assert!(select.contains("background: root.on-chrome ? Theme.chrome-field : Theme.field;"));
    assert!(select.contains("chevron-down.svg"));
}

#[test]
fn the_instruments_are_solid_silhouettes() {
    for svg in [
        "electric_guitar.svg",
        "acoustic_guitar.svg",
        "bass.svg",
        "voice.svg",
        "keys.svg",
        "drums.svg",
        "generic.svg",
    ] {
        let art = repo_asset(&format!("instruments/{svg}"));
        assert!(
            !art.contains("stroke-width=\"1.5\""),
            "{svg} is still the old outline"
        );
        assert!(
            art.contains("<mask"),
            "{svg} cuts its details out of the body"
        );
    }
}
