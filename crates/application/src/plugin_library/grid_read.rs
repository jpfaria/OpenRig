//! Responsibility: shows a capture plugin's manifest as the editor grid.

use std::path::Path;

use plugin_loader::manifest::{GridCapture, GridParameter, ParameterValue};

use super::capture_names::capture_names;
use super::grid_types::{ColumnKind, EditorGrid, GridColumn, GridRow};

/// One row per capture, one column per parameter, with each capture's
/// original name beside it.
pub fn read_grid(
    package_root: &Path,
    parameters: &[GridParameter],
    captures: &[GridCapture],
) -> EditorGrid {
    let names = capture_names(package_root, captures);
    let columns: Vec<GridColumn> = parameters
        .iter()
        .map(|parameter| GridColumn {
            name: parameter.name.clone(),
            display_name: parameter.display_name.clone(),
            kind: column_kind(&parameter.values),
        })
        .collect();
    let rows = captures
        .iter()
        .zip(names)
        .map(|(capture, source_name)| GridRow {
            file: capture.file.clone(),
            source_name,
            cells: parameters
                .iter()
                .map(|p| {
                    capture
                        .values
                        .get(&p.name)
                        .map(cell_text)
                        .unwrap_or_default()
                })
                .collect(),
        })
        .collect();
    EditorGrid { columns, rows }
}

/// A knob over numbers, a switch over booleans, a choice otherwise — the
/// control the block editor shows for the same values.
pub fn column_kind(values: &[ParameterValue]) -> ColumnKind {
    if values.is_empty() {
        ColumnKind::Choice
    } else if values
        .iter()
        .all(|v| matches!(v, ParameterValue::Number(_)))
    {
        ColumnKind::Knob
    } else if values.iter().all(|v| matches!(v, ParameterValue::Bool(_))) {
        ColumnKind::Switch
    } else {
        ColumnKind::Choice
    }
}

pub fn cell_text(value: &ParameterValue) -> String {
    match value {
        ParameterValue::Text(text) => text.clone(),
        ParameterValue::Number(n) => n.to_string(),
        ParameterValue::Bool(b) => b.to_string(),
    }
}
