//! Responsibility: describes the capture grid the plugin editor shows.
//!
//! One row per capture file, one column per parameter. A cell is text: a
//! knob holds a number, a switch `true` or `false`, a choice any word.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EditorGrid {
    pub columns: Vec<GridColumn>,
    pub rows: Vec<GridRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GridColumn {
    /// The parameter's key in a project file.
    pub name: String,
    /// The label on screen; `None` shows the name with a capital letter.
    pub display_name: Option<String>,
    pub kind: ColumnKind,
}

/// How a parameter is played: a knob over numbers, a list of choices or an
/// on/off switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColumnKind {
    Knob,
    Choice,
    Switch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GridRow {
    /// The capture file: inside the package when editing, anywhere on disk
    /// when creating a plugin.
    pub file: PathBuf,
    /// The capture's original name (its TONE3000 name, else the file name).
    /// Shown only; a save ignores it.
    #[serde(default)]
    pub source_name: String,
    /// One cell per column, in column order.
    pub cells: Vec<String>,
}

/// What a created plugin plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaptureBackend {
    Nam,
    Ir,
}
