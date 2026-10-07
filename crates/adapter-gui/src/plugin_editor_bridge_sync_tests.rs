use std::path::PathBuf;

use application::plugin_library::{ColumnKind, EditorGrid, GridColumn, GridRow};
use slint::{Global, Model};

use super::{mode_index, set_editor_view, version_options, EditorChrome};
use crate::plugin_editor_draft::{EditorDraft, EditorMode};
use crate::{PluginEditorBridge, PluginEditorWindow};

fn draft() -> EditorDraft {
    let mut draft = EditorDraft::create();
    draft.mode = EditorMode::Edit {
        plugin_id: "plexi".into(),
    };
    draft.grid = EditorGrid {
        columns: vec![GridColumn {
            name: "gain".into(),
            display_name: None,
            kind: ColumnKind::Knob,
        }],
        rows: vec![GridRow {
            file: PathBuf::from("captures/plexi_g5.nam"),
            source_name: "plexi g5".into(),
            cells: vec!["5".into()],
        }],
    };
    draft.versions = vec![1, 2, 3];
    draft
}

#[test]
fn the_draft_reaches_the_window() {
    i_slint_backend_testing::init_no_event_loop();
    let w = PluginEditorWindow::new().unwrap();
    let chrome = EditorChrome {
        subject: "Plexi",
        tone3000: true,
        busy: true,
        error: "bad name",
    };
    set_editor_view(&PluginEditorBridge::get(&w), &draft(), &chrome);
    let bridge = PluginEditorBridge::get(&w);
    assert_eq!(bridge.get_mode(), 0);
    assert_eq!(bridge.get_subject(), "Plexi");
    assert!(bridge.get_tone3000() && bridge.get_busy());
    assert_eq!(bridge.get_error(), "bad name");
    let column = bridge.get_columns().row_data(0).unwrap();
    assert_eq!((column.name.as_str(), column.kind), ("gain", 0));
    let row = bridge.get_rows().row_data(0).unwrap();
    assert_eq!(row.source_name, "plexi g5");
    assert_eq!(row.cells.row_data(0).unwrap(), "5");
    assert_eq!(bridge.get_versions().row_count(), 3);
}

#[test]
fn versions_read_newest_first() {
    let keys: Vec<String> = version_options(&[1, 2, 3])
        .iter()
        .map(|o| o.key.to_string())
        .collect();
    assert_eq!(keys, vec!["3", "2", "1"]);
}

#[test]
fn each_mode_has_its_code() {
    assert_eq!(mode_index(&EditorMode::Create), 1);
    assert_eq!(
        mode_index(&EditorMode::Naming {
            tone_id: 4,
            plugin_id: "tone3000_4".into()
        }),
        2
    );
}
