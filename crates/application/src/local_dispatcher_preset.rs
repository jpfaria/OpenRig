//! `Command::SaveChainPreset` / `Command::DeleteChainPreset` — the
//! dispatcher owns the preset-file I/O so MCP / gRPC / MIDI clients
//! produce the same effect as the GUI dispatching the Command (the
//! whole point of #555: GUI sem regra de negócio).

use anyhow::Result;

use infra_yaml::{save_chain_preset_file, ChainBlocksPreset};
use project::block::{AudioBlock, AudioBlockKind};

use crate::command::{ChainCommand, Command};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::preset_file::preset_save_path;

impl LocalDispatcher {
    /// `Command::SaveChainPreset` / `DeleteChainPreset` — resolves the
    /// on-disk path from the attached `presets_path` and performs the
    /// file operation, then emits the corresponding event so MCP /
    /// MIDI / GUI observers can refresh.
    ///
    /// `presets_path` is attached by the session bootstrap via
    /// [`LocalDispatcher::attach_presets_path`]. Until that is called,
    /// preset Commands skip the I/O step (only emit the event) so
    /// dispatcher unit tests that don't care about preset I/O keep
    /// working without a tempdir.
    pub(crate) fn handle_chain_preset(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            // #693: the preset snapshot is built in memory here; every
            // disk touch runs on the persist worker so the dispatching
            // thread never waits on I/O. Errors surface via the
            // non-blocking logger.
            Command::Chain(ChainCommand::SaveChainPreset { chain, name }) => {
                if let Some(dir) = self.presets_path.borrow().clone() {
                    let project = self.project.borrow();
                    let target =
                        project
                            .chains
                            .iter()
                            .find(|c| c.id == chain)
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                "Command::SaveChainPreset: chain {chain:?} not found in project"
                            )
                            })?;
                    let path = preset_save_path(&dir, &name);
                    let preset = ChainBlocksPreset {
                        id: preset_id_from_path(&path),
                        name: target.description.clone(),
                        volume: target.volume,
                        instrument: target.instrument.clone(),
                        blocks: strip_io_blocks(&target.blocks),
                    };
                    crate::persist_worker::run(move || {
                        if let Err(e) = std::fs::create_dir_all(&dir) {
                            log::error!("failed to create presets dir {dir:?}: {e}");
                            return;
                        }
                        if let Err(e) = save_chain_preset_file(&path, &preset) {
                            log::error!("failed to write preset file {path:?}: {e}");
                        }
                    });
                }
                Ok(vec![Event::ChainPresetSaved { name }])
            }
            Command::Chain(ChainCommand::DeleteChainPreset { name }) => {
                if let Some(dir) = self.presets_path.borrow().as_ref() {
                    let path = preset_save_path(dir, &name);
                    crate::persist_worker::run(move || {
                        // missing file is a silent no-op — same UX as the
                        // GUI's previous fs::remove_file path. Observers
                        // still get the event so they can refresh the
                        // picker.
                        if path.exists() {
                            if let Err(e) = std::fs::remove_file(&path) {
                                log::error!("failed to remove preset file {path:?}: {e}");
                            }
                        }
                    });
                }
                Ok(vec![Event::ChainPresetDeleted { name }])
            }
            other => {
                unreachable!("handle_chain_preset received non-preset command: {other:?}")
            }
        }
    }
}

/// Filter out input/output blocks before serialising — the chain's
/// I/O wiring is project state, not preset state. Mirrors the
/// behaviour of `adapter-gui::chain_preset_wiring::strip_io_blocks`
/// (which now only filters in-memory after the load path; the disk
/// side is owned here).
fn strip_io_blocks(blocks: &[AudioBlock]) -> Vec<AudioBlock> {
    blocks
        .iter()
        .filter(|b| !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
        .cloned()
        .collect()
}

/// Derive a stable preset id from the on-disk filename. The id is the
/// file stem — mirrors the load path's expectation.
fn preset_id_from_path(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
#[path = "local_dispatcher_preset_tests.rs"]
mod tests;
