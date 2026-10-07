use std::path::PathBuf;

use application::command::{Command, PluginLibraryCommand, Tone3000Command};
use application::plugin_library::{
    CaptureBackend, ColumnKind, EditorGrid, GridColumn, GridRow, PluginOrigin,
};
use application::query_plugin_library::PluginGridView;
use application::tone3000::install_pending::PendingInstall;
use application::tone3000::Tone3000BlockType;

use super::*;

fn grid() -> EditorGrid {
    EditorGrid {
        columns: vec![GridColumn {
            name: "gain".into(),
            display_name: None,
            kind: ColumnKind::Knob,
        }],
        rows: vec![
            GridRow {
                file: "captures/0.nam".into(),
                source_name: "Gain 2".into(),
                cells: vec!["2".into()],
            },
            GridRow {
                file: "captures/1.nam".into(),
                source_name: "Gain 8".into(),
                cells: vec!["8".into()],
            },
        ],
    }
}

fn edited() -> EditorDraft {
    EditorDraft::edit(PluginGridView {
        plugin_id: "plexi".into(),
        origin: PluginOrigin::PluginsFolder,
        editable: true,
        versions: vec![1, 2],
        grid: Some(grid()),
    })
    .expect("a capture plugin opens")
}

#[test]
fn a_plugin_without_a_grid_does_not_open() {
    let view = PluginGridView {
        plugin_id: "lv2".into(),
        origin: PluginOrigin::PluginsFolder,
        editable: false,
        versions: vec![],
        grid: None,
    };
    assert!(EditorDraft::edit(view).is_none());
}

#[test]
fn an_edited_plugin_keeps_its_versions() {
    let draft = edited();
    assert_eq!(draft.versions, vec![1, 2]);
    assert_eq!(draft.plugin_id(), Some("plexi"));
}

#[test]
fn a_cell_edit_changes_only_that_cell() {
    let mut draft = edited();
    draft.set_cell(1, 0, "9");
    draft.set_cell(5, 0, "ignored");
    assert_eq!(draft.grid.rows[1].cells, vec!["9"]);
    assert_eq!(draft.grid.rows[0].cells, vec!["2"]);
}

#[test]
fn a_column_is_renamed_without_surrounding_spaces() {
    let mut draft = edited();
    draft.rename_column(0, " drive ");
    assert_eq!(draft.grid.columns[0].name, "drive");
}

#[test]
fn the_kind_segments_map_in_order() {
    let mut draft = edited();
    draft.set_kind(0, 2);
    assert_eq!(draft.grid.columns[0].kind, ColumnKind::Switch);
    draft.set_kind(0, 1);
    assert_eq!(draft.grid.columns[0].kind, ColumnKind::Choice);
    draft.set_kind(0, 7);
    assert_eq!(draft.grid.columns[0].kind, ColumnKind::Choice);
}

#[test]
fn an_added_column_has_a_free_name_and_an_empty_cell_per_row() {
    let mut draft = edited();
    draft.add_column();
    draft.add_column();
    let names: Vec<&str> = draft.grid.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["gain", "param_1", "param_2"]);
    assert_eq!(draft.grid.columns[1].kind, ColumnKind::Choice);
    assert!(draft.grid.rows.iter().all(|r| r.cells.len() == 3));
    assert_eq!(draft.grid.rows[0].cells[1], "");
}

#[test]
fn a_removed_column_takes_its_cells() {
    let mut draft = edited();
    draft.add_column();
    draft.set_cell(0, 1, "x");
    draft.remove_column(0);
    assert_eq!(draft.grid.columns.len(), 1);
    assert_eq!(draft.grid.rows[0].cells, vec!["x"]);
    draft.remove_column(9);
    assert_eq!(draft.grid.columns.len(), 1);
}

#[test]
fn the_first_files_of_a_new_plugin_open_a_choice_column_named_per_file() {
    let mut draft = EditorDraft::create();
    draft.add_files(vec![
        PathBuf::from("/caps/Clean.nam"),
        PathBuf::from("/caps/Crunch.nam"),
        PathBuf::from("/caps/Clean.nam"),
    ]);
    assert_eq!(draft.grid.columns[0].name, FIRST_COLUMN);
    assert_eq!(draft.grid.columns[0].kind, ColumnKind::Choice);
    let cells: Vec<&str> = draft
        .grid
        .rows
        .iter()
        .map(|r| r.cells[0].as_str())
        .collect();
    assert_eq!(cells, vec!["Clean", "Crunch"]);
    assert_eq!(draft.grid.rows[1].source_name, "Crunch");
}

#[test]
fn files_added_later_get_an_empty_cell_for_the_other_columns() {
    let mut draft = EditorDraft::create();
    draft.add_files(vec![PathBuf::from("/caps/a.wav")]);
    draft.add_column();
    draft.add_files(vec![PathBuf::from("/caps/b.wav")]);
    assert_eq!(draft.grid.rows[1].cells, vec!["b", ""]);
    draft.remove_row(0);
    assert_eq!(draft.grid.rows.len(), 1);
}

#[test]
fn saving_an_edit_saves_the_plugins_parameters() {
    let mut draft = edited();
    draft.set_cell(0, 0, "3");
    match draft.save_command() {
        Command::PluginLibrary(PluginLibraryCommand::SavePluginParameters { plugin_id, grid }) => {
            assert_eq!(plugin_id, "plexi");
            assert_eq!(grid.rows[0].cells, vec!["3"]);
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(draft.cancel_command().is_none());
}

#[test]
fn saving_a_new_plugin_creates_it_with_its_fields() {
    let mut draft = EditorDraft::create();
    draft.create.display_name = "  My Amp ".into();
    draft.create.brand = "   ".into();
    draft.set_block_type(3);
    draft.set_backend(1);
    draft.add_files(vec![PathBuf::from("/caps/a.wav")]);
    match draft.save_command() {
        Command::PluginLibrary(PluginLibraryCommand::CreatePlugin {
            display_name,
            brand,
            block_type,
            backend,
            grid,
        }) => {
            assert_eq!(display_name, "My Amp");
            assert_eq!(brand, None);
            assert_eq!(block_type, Tone3000BlockType::Cab);
            assert_eq!(backend, CaptureBackend::Ir);
            assert_eq!(grid.rows.len(), 1);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn naming_a_tone_finishes_its_install_and_closing_drops_it() {
    let pending = PendingInstall {
        tone_id: 42,
        plugin_id: "tone3000_42".into(),
        dir: "/plugins/nam/.pending-tone3000_42".into(),
        grid: grid(),
    };
    let draft = EditorDraft::naming(&pending);
    assert!(draft.plugin_id().is_none());
    assert!(matches!(
        draft.save_command(),
        Command::Tone3000(Tone3000Command::FinishTone3000Install { tone_id: 42, .. })
    ));
    assert!(matches!(
        draft.cancel_command(),
        Some(Command::Tone3000(Tone3000Command::CancelTone3000Install {
            tone_id: 42
        }))
    ));
}
