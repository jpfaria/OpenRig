//! Responsibility: holds the capture grid the user is editing in the plugin editor.
//!
//! Pure: every edit of the window lands here, and a save turns the draft
//! into the one command its mode sends. Editing a plugin, creating one and
//! naming a TONE3000 tone before it installs share this draft.

use std::path::PathBuf;

use application::command::{Command, PluginLibraryCommand, Tone3000Command};
use application::plugin_library::{CaptureBackend, ColumnKind, EditorGrid, GridColumn, GridRow};
use application::query_plugin_library::PluginGridView;
use application::tone3000::install_pending::PendingInstall;
use application::tone3000::Tone3000BlockType;

/// The column a new plugin's first captures get, one choice per file.
pub(crate) const FIRST_COLUMN: &str = "capture";

/// The block types a new plugin can be, in the window's segment order.
pub(crate) const CREATE_BLOCK_TYPES: [Tone3000BlockType; 5] = [
    Tone3000BlockType::Amp,
    Tone3000BlockType::Preamp,
    Tone3000BlockType::GainPedal,
    Tone3000BlockType::Cab,
    Tone3000BlockType::Body,
];

/// The column kinds, in the window's segment order.
pub(crate) const COLUMN_KINDS: [ColumnKind; 3] =
    [ColumnKind::Knob, ColumnKind::Choice, ColumnKind::Switch];

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EditorMode {
    Edit { plugin_id: String },
    Create,
    Naming { tone_id: u64, plugin_id: String },
}

/// What only a new plugin asks for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CreateFields {
    pub display_name: String,
    pub brand: String,
    pub block_type: Tone3000BlockType,
    pub backend: CaptureBackend,
}

impl Default for CreateFields {
    fn default() -> Self {
        Self {
            display_name: String::new(),
            brand: String::new(),
            block_type: Tone3000BlockType::Amp,
            backend: CaptureBackend::Nam,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditorDraft {
    pub mode: EditorMode,
    pub grid: EditorGrid,
    /// The kept versions, oldest first; only an edited plugin has them.
    pub versions: Vec<u32>,
    pub create: CreateFields,
}

impl EditorDraft {
    /// The draft of an owned plugin; `None` when it has no capture grid.
    pub(crate) fn edit(view: PluginGridView) -> Option<Self> {
        Some(Self {
            mode: EditorMode::Edit {
                plugin_id: view.plugin_id,
            },
            grid: view.grid?,
            versions: view.versions,
            create: CreateFields::default(),
        })
    }

    pub(crate) fn create() -> Self {
        Self {
            mode: EditorMode::Create,
            grid: EditorGrid {
                columns: vec![],
                rows: vec![],
            },
            versions: vec![],
            create: CreateFields::default(),
        }
    }

    pub(crate) fn naming(pending: &PendingInstall) -> Self {
        Self {
            mode: EditorMode::Naming {
                tone_id: pending.tone_id,
                plugin_id: pending.plugin_id.clone(),
            },
            grid: pending.grid.clone(),
            versions: vec![],
            create: CreateFields::default(),
        }
    }

    pub(crate) fn set_cell(&mut self, row: usize, column: usize, value: &str) {
        if let Some(cell) = self
            .grid
            .rows
            .get_mut(row)
            .and_then(|r| r.cells.get_mut(column))
        {
            *cell = value.to_string();
        }
    }

    pub(crate) fn rename_column(&mut self, column: usize, name: &str) {
        if let Some(c) = self.grid.columns.get_mut(column) {
            c.name = name.trim().to_string();
        }
    }

    pub(crate) fn set_kind(&mut self, column: usize, kind_index: i32) {
        let kind = usize::try_from(kind_index)
            .ok()
            .and_then(|i| COLUMN_KINDS.get(i));
        if let (Some(c), Some(kind)) = (self.grid.columns.get_mut(column), kind) {
            c.kind = *kind;
        }
    }

    /// Adds a choice column with a free name and an empty cell per row.
    pub(crate) fn add_column(&mut self) {
        let taken = |name: &str| self.grid.columns.iter().any(|c| c.name == name);
        let name = (1..)
            .map(|n| format!("param_{n}"))
            .find(|name| !taken(name))
            .unwrap_or_default();
        self.grid.columns.push(GridColumn {
            name,
            display_name: None,
            kind: ColumnKind::Choice,
        });
        for row in &mut self.grid.rows {
            row.cells.push(String::new());
        }
    }

    pub(crate) fn remove_column(&mut self, column: usize) {
        if column >= self.grid.columns.len() {
            return;
        }
        self.grid.columns.remove(column);
        for row in &mut self.grid.rows {
            if column < row.cells.len() {
                row.cells.remove(column);
            }
        }
    }

    /// Adds one row per new capture file. The first files of a new plugin
    /// open a choice column named after each file.
    pub(crate) fn add_files(&mut self, files: Vec<PathBuf>) {
        if self.grid.columns.is_empty() {
            self.grid.columns.push(GridColumn {
                name: FIRST_COLUMN.into(),
                display_name: None,
                kind: ColumnKind::Choice,
            });
            for row in &mut self.grid.rows {
                row.cells = vec![row.source_name.clone()];
            }
        }
        for file in files {
            if self.grid.rows.iter().any(|r| r.file == file) {
                continue;
            }
            let source_name = file
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut cells = vec![String::new(); self.grid.columns.len()];
            cells[0] = source_name.clone();
            self.grid.rows.push(GridRow {
                file,
                source_name,
                cells,
            });
        }
    }

    pub(crate) fn remove_row(&mut self, row: usize) {
        if row < self.grid.rows.len() {
            self.grid.rows.remove(row);
        }
    }

    pub(crate) fn set_block_type(&mut self, index: i32) {
        if let Some(t) = usize::try_from(index)
            .ok()
            .and_then(|i| CREATE_BLOCK_TYPES.get(i))
        {
            self.create.block_type = *t;
        }
    }

    pub(crate) fn set_backend(&mut self, index: i32) {
        self.create.backend = if index == 1 {
            CaptureBackend::Ir
        } else {
            CaptureBackend::Nam
        };
    }

    /// The command a save sends.
    pub(crate) fn save_command(&self) -> Command {
        let grid = self.grid.clone();
        match &self.mode {
            EditorMode::Edit { plugin_id } => {
                Command::PluginLibrary(PluginLibraryCommand::SavePluginParameters {
                    plugin_id: plugin_id.clone(),
                    grid,
                })
            }
            EditorMode::Create => Command::PluginLibrary(PluginLibraryCommand::CreatePlugin {
                display_name: self.create.display_name.trim().to_string(),
                brand: Some(self.create.brand.trim().to_string()).filter(|b| !b.is_empty()),
                block_type: self.create.block_type,
                backend: self.create.backend,
                grid,
            }),
            EditorMode::Naming { tone_id, .. } => {
                Command::Tone3000(Tone3000Command::FinishTone3000Install {
                    tone_id: *tone_id,
                    grid,
                })
            }
        }
    }

    /// The command closing the window without saving sends: a tone that
    /// waits for names is dropped.
    pub(crate) fn cancel_command(&self) -> Option<Command> {
        match &self.mode {
            EditorMode::Naming { tone_id, .. } => {
                Some(Command::Tone3000(Tone3000Command::CancelTone3000Install {
                    tone_id: *tone_id,
                }))
            }
            _ => None,
        }
    }

    /// The plugin this draft edits, when it is one already on disk.
    pub(crate) fn plugin_id(&self) -> Option<&str> {
        match &self.mode {
            EditorMode::Edit { plugin_id } => Some(plugin_id),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "plugin_editor_draft_tests.rs"]
mod tests;
