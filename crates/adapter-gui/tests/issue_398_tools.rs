//! #398 — the tool windows (tuner, spectrum, metronome, drums, backing
//! tracks): a dark tool bar with footswitch pills, then the window's body on
//! the well. Every readout is a dark LCD glass: amber digits for the tuner,
//! the metronome, the drums and the player; a teal scope for the spectrum.

use std::path::PathBuf;

fn ui(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn every_tool_window_opens_with_the_dark_bar_and_pill_switches() {
    for page in [
        "pages/tuner_window.slint",
        "pages/spectrum_window.slint",
        "pages/metronome_window.slint",
        "pages/drums_window.slint",
    ] {
        let src = ui(page);
        assert!(src.contains("ToolBar {"), "{page}: the dark tool bar");
        assert!(src.contains("PillSwitch {"), "{page}: POWER is a pill");
        assert!(!src.contains("footswitch.png"), "{page}: no sprite switch");
        assert!(
            !src.contains("toggleswitch.png"),
            "{page}: no sprite toggle"
        );
        assert!(
            src.contains("Theme.well"),
            "{page}: the body sits on the well"
        );
    }
    let player = ui("pages/player_window.slint");
    assert!(player.contains("ToolBar {"));
    let pill = ui("components/pill_switch.slint");
    assert!(
        pill.contains("in property <bool> enabled"),
        "MUTE dims while off"
    );
}

#[test]
fn the_readouts_are_dark_lcd_glass() {
    let glass = ui("components/lcd_glass.slint");
    assert!(glass.starts_with("// Responsibility: "));
    assert!(glass.contains("@children"));
    let theme = ui("theme.slint");
    for token in [
        "lcd-glow",
        "lcd-dim",
        "lcd-faint",
        "lcd-lamp",
        "scope-mid",
        "scope-axis",
    ] {
        assert!(
            theme.contains(&format!("out property <color> {token}:")),
            "theme token {token}"
        );
    }
    assert!(
        !theme.contains("needle-near"),
        "the old amber-panel LCD is gone"
    );
    let tuner = ui("pages/tuner_window.slint");
    assert!(tuner.contains("LcdGlass {"));
    assert!(tuner.contains("Theme.lcd-glow"));
    assert!(
        tuner.contains("Theme.lcd-in"),
        "in tune turns the needle green"
    );
    let spectrum = ui("pages/spectrum_window.slint");
    assert!(spectrum.contains("LcdGlass {"));
    assert!(spectrum.contains("Theme.scope-mid"));
    assert!(spectrum.contains("@tr(\"label-spectrum-bands\""));
}

#[test]
fn the_metronome_sets_its_tempo_on_a_knob() {
    let page = ui("pages/metronome_window.slint");
    assert!(page.contains("LcdGlass {"));
    assert!(page.contains("path: \"metronome.bpm\";"), "a BPM knob");
    assert!(page.contains("TapKey {"), "TAP is a white key");
    let controls = ui("components/metronome_controls.slint");
    assert!(
        controls.contains("Theme.lcd-lamp"),
        "unlit lamps on the glass"
    );
    assert!(!controls.contains("PowerFootSwitch"));
    assert!(!controls.contains("toggleswitch.png"));
    assert!(!controls.contains("led-red.png"));
}

#[test]
fn the_drums_fill_is_a_round_metal_button() {
    let page = ui("pages/drums_window.slint");
    assert!(page.contains("LcdGlass {"));
    let controls = ui("components/drums_controls.slint");
    assert!(!controls.contains("footswitch.png"));
    assert!(controls.contains("Gear.switch-hi"), "the metal cap");
    assert!(controls.contains("Theme.field-line"), "the closed field");
}

#[test]
fn the_player_is_an_lcd_over_round_transport_buttons() {
    let page = ui("pages/player_window.slint");
    assert!(page.contains("LcdGlass {"));
    assert!(
        page.contains("PlayerBridge.position-label")
            && page.contains("PlayerBridge.duration-label"),
        "the time large, the total small beside it"
    );
    let button = ui("components/looper_controls.slint");
    assert!(
        button.contains("border-radius: 20px;"),
        "round 40px buttons"
    );
    assert!(button.contains("Theme.field-line"));
    let transport = ui("components/player_transport.slint");
    assert!(
        transport.contains("Theme.in"),
        "PLAY lights in the input colour"
    );
}

fn slint_files(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            slint_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "slint") {
            out.push(path);
        }
    }
}

#[test]
fn no_screen_draws_a_sprite_any_more() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut files = Vec::new();
    slint_files(&root, &mut files);
    for file in files {
        let src = std::fs::read_to_string(&file).unwrap();
        assert!(
            !src.contains("assets/sprites/"),
            "{}: every control is drawn, no sprite sheet",
            file.display()
        );
    }
    assert!(
        !root.join("assets/sprites").exists(),
        "the sprite folder is gone"
    );
}

#[test]
fn the_eq_bands_are_white_caps_on_a_thin_groove() {
    let eq = ui("pages/eq_widget.slint");
    assert!(eq.contains("background: Theme.groove;"), "the thin groove");
    assert!(
        eq.contains("Theme.c-filter"),
        "boost or cut in the filter colour"
    );
    assert!(eq.contains("Gear.white"), "the white fader cap");
    assert!(
        eq.contains("background: Theme.well;"),
        "the bands sit on the well"
    );
}
