//! #398 — the audio interface offers 192 kHz and a 2048-frame buffer, and the
//! add-block picker draws its icons large enough to read.

use std::path::PathBuf;

fn ui(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_audio_options_include_192k_and_2048() {
    let app = ui("app-window.slint");
    assert!(
        app.contains("\"192000\"]"),
        "sample-rate options miss 192000"
    );
    assert!(app.contains("\"2048\"]"), "buffer-size options miss 2048");
    for file in [
        "pages/settings/section_system_audio.slint",
        "widgets/device_row.slint",
    ] {
        let src = ui(file);
        assert!(
            src.contains("value == \"192000\""),
            "{file} cannot select 192000"
        );
        assert!(
            src.contains("value == \"2048\""),
            "{file} cannot select 2048"
        );
    }
}

#[test]
fn the_block_type_icons_are_at_least_64px() {
    let src = ui("components/block_value_widgets.slint");
    let icon = src
        .lines()
        .find_map(|l| l.trim().strip_prefix("out property <length> icon: "))
        .expect("BlockTypeGrid.icon is declared");
    let px: f32 = icon.trim_end_matches("px;").parse().expect("icon in px");
    assert!(px >= 64.0, "icon is {px}px");
    let card = &src[src.find("export component BlockTypeCard").unwrap()..];
    assert_eq!(
        card.matches("width: BlockTypeGrid.icon").count(),
        2,
        "both icons use the token"
    );
    for file in [
        "pages/project_chains.slint",
        "pages/compact_chain_view.slint",
    ] {
        assert!(
            ui(file).contains("BlockTypeGrid.step"),
            "{file} grid ignores the card size"
        );
    }
}
