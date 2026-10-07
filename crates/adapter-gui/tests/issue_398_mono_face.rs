//! #398 — values and readouts (knob values, dB, times, channel names, the LCD
//! lines) are set in JetBrains Mono, the face the approved mockup uses for
//! them; a CJK or Devanagari locale keeps its own script face there, as it
//! does for display text.

use adapter_gui::{Locale, TunerWindow};
use slint::Global;
use std::path::Path;

/// The files that draw a readout the mockup sets in `var(--mono)`.
const MONO_READOUTS: &[&str] = &[
    "components/panel_knob.slint",
    "components/mixer_strip.slint",
    "components/chain_latency_badge.slint",
    "components/app_version_badge.slint",
    "components/channel_picker.slint",
    "components/tone_doctor_metric.slint",
    "components/looper_row.slint",
    "components/looper_controls.slint",
    "components/looper_preset_picker.slint",
    "components/gear_art.slint",
    "pages/tuner_window.slint",
    "pages/spectrum_window.slint",
    "pages/metronome_window.slint",
    "pages/drums_window.slint",
    "pages/player_window.slint",
    "pages/project_launcher.slint",
    "pages/eq_widget.slint",
    "pages/settings/section_project_meta.slint",
];

fn ui(file: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("ui").join(file))
        .unwrap_or_else(|e| panic!("{file}: {e}"))
}

#[test]
fn a_latin_locale_sets_readouts_in_jetbrains_mono() {
    i_slint_backend_testing::init_no_event_loop();
    let w = TunerWindow::new().unwrap();
    assert_eq!(Locale::get(&w).get_mono_font_family(), "JetBrains Mono");
}

#[test]
fn a_cjk_locale_keeps_its_script_face_for_readouts() {
    i_slint_backend_testing::init_no_event_loop();
    let w = TunerWindow::new().unwrap();
    Locale::get(&w).set_font_family("Hiragino Sans".into());
    assert_eq!(Locale::get(&w).get_mono_font_family(), "Hiragino Sans");
}

#[test]
fn the_mono_face_ships_with_the_app() {
    assert!(
        ui("app-window.slint").contains("import \"fonts/JetBrainsMono/JetBrainsMono-Medium.ttf\";")
    );
    assert!(Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("ui/fonts/JetBrainsMono/OFL.txt")
        .exists());
}

#[test]
fn every_readout_the_mockup_sets_in_mono_uses_the_mono_face() {
    let missing: Vec<_> = MONO_READOUTS
        .iter()
        .filter(|f| !ui(f).contains("Locale.mono-font-family"))
        .collect();
    assert!(missing.is_empty(), "still in the UI face: {missing:?}");
}
