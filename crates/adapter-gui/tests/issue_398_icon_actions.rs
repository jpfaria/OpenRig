//! #398 — an action whose icon already says what it does (save is a floppy,
//! delete a bin, add a plus, cancel a cross) shows the icon alone; its name
//! stays as the hover label and the accessible label. Only actions no icon
//! can say (overwrite, reset to default, trim, crop, …) keep their words. The
//! split lanes are told apart by colour, never by a written "PATH A".

use std::path::{Path, PathBuf};

fn ui_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui")
}

fn ui(rel: &str) -> String {
    let path = ui_dir().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

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

/// Actions an icon says on its own.
const OBVIOUS: &[&str] = &[
    "btn-cancel",
    "btn-close",
    "btn-save",
    "btn-save-chain",
    "btn-create-chain",
    "btn-save-project",
    "btn-save-audio-settings",
    "looper-take-save",
    "btn-delete",
    "btn-remove",
    "btn-ok",
    "btn-add",
    "btn-new-binding",
    "btn-add-path",
    "btn-add-input",
    "btn-add-output",
    "looper-add",
    "btn-refresh-devices",
    "btn-reload-plugin-catalog",
    "btn-choose-path",
    "looper-play-all",
    "looper-stop-all",
    "looper-editor-play",
    "looper-editor-stop",
    "looper-editor-undo",
    "looper-editor-redo",
    "looper-editor-close",
    "btn-wizard-back",
    "btn-wizard-next",
    "btn-wizard-finish",
    "btn-tone3000-uninstall",
    "btn-tone3000-previous-page",
    "btn-tone3000-next-page",
];

/// Actions no icon says; they keep their words.
const WORDED: &[&str] = &[
    "btn-tone3000-uninstall-confirm",
    "btn-tone3000-key-save",
    "btn-tone3000-key-clear",
    "btn-tone3000-open-settings",
    "btn-tone3000-search",
    "btn-tone3000-install",
    "btn-tone3000-retry",
    "btn-overwrite",
    "btn-reset-path",
    "looper-editor-fit",
    "looper-editor-trim",
    "looper-editor-crop",
    "looper-editor-cut",
    "tone-doctor-diagnose",
    "tone-doctor-diagnosing",
    "tone-doctor-rerun",
    "btn-wizard-skip",
];

const BUTTONS: &[&str] = &["FormButton {", "EditorButton {", "PanelActionButton {"];

/// Every `<Button> { … }` block of a file, with the line it starts on.
fn button_blocks(src: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for open in BUTTONS {
        let mut from = 0;
        while let Some(at) = src[from..].find(open) {
            let start = from + at;
            let prev = src[..start].chars().last().unwrap_or(' ');
            from = start + open.len();
            if prev.is_alphanumeric() || prev == '-' {
                continue;
            }
            let mut depth = 1;
            let mut end = from;
            for (i, c) in src[from..].char_indices() {
                depth += (c == '{') as i32 - (c == '}') as i32;
                if depth == 0 {
                    end = from + i + 1;
                    break;
                }
            }
            let line = src[..start].matches('\n').count() + 1;
            out.push((line, src[start..end].to_string()));
        }
    }
    out
}

fn label_keys(block: &str) -> Vec<String> {
    let label = block
        .find("label:")
        .map(|at| &block[at..at + block[at..].find(';').unwrap()])
        .unwrap_or("");
    label
        .split("@tr(\"")
        .skip(1)
        .map(|s| s[..s.find('"').unwrap()].to_string())
        .collect()
}

#[test]
fn the_button_components_can_show_their_icon_alone() {
    for file in [
        "components/form_button.slint",
        "components/editor_button.slint",
        "components/panel_action_button.slint",
    ] {
        let src = ui(file);
        assert!(
            src.contains("in property <bool> icon-only"),
            "{file} has no icon-only form"
        );
        assert!(
            src.contains("IconTooltip {"),
            "{file}: an icon-only button names itself on hover"
        );
        assert!(
            src.contains("accessible-label: root.label"),
            "{file}: the name stays for screen readers"
        );
    }
}

#[test]
fn every_obvious_action_is_an_icon_and_every_other_keeps_its_words() {
    let mut files = Vec::new();
    app_slint_files(&ui_dir(), &mut files);
    let mut seen = 0;
    for path in files {
        let src = std::fs::read_to_string(&path).unwrap();
        let rel = path.strip_prefix(ui_dir()).unwrap().display().to_string();
        for (line, block) in button_blocks(&src) {
            let keys = label_keys(&block);
            if keys.is_empty() {
                continue;
            }
            seen += 1;
            let obvious = keys.iter().all(|k| OBVIOUS.contains(&k.as_str()));
            let worded = keys.iter().all(|k| WORDED.contains(&k.as_str()));
            assert!(
                obvious || worded,
                "{rel}:{line}: classify {keys:?} as an icon action or a worded one"
            );
            let icon_only = block.contains("icon-only: true");
            if obvious {
                assert!(
                    icon_only,
                    "{rel}:{line}: {keys:?} is said by its icon alone"
                );
                assert!(
                    block.contains("icon:"),
                    "{rel}:{line}: {keys:?} has no icon"
                );
            } else {
                assert!(!icon_only, "{rel}:{line}: {keys:?} keeps its words");
            }
        }
    }
    assert!(seen > 40, "only {seen} buttons found");
}

#[test]
fn the_split_lanes_are_told_apart_by_colour_not_by_a_written_path_name() {
    let card = ui("components/graph_node_card.slint");
    assert!(
        !card.contains("graph-lane-tag"),
        "the graph block carries no PATH tag"
    );
    let row = ui("pages/compact_block_row.slint");
    for key in [
        "compact-lane-path",
        "compact-lane-mix",
        "compact-lane-split",
    ] {
        assert!(
            !row.contains(key),
            "the compact lane bar still writes {key}"
        );
    }
    assert!(
        row.contains("graph-split.svg"),
        "a path row shows the split icon"
    );
    assert!(
        row.contains("graph-mix.svg"),
        "the mix row shows the mix icon"
    );
}
