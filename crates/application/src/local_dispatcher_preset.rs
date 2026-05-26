//! `Command::SaveChainPreset` / `Command::DeleteChainPreset` — the
//! dispatcher owns the preset-file I/O so MCP / gRPC / MIDI clients
//! produce the same effect as the GUI dispatching the Command (the
//! whole point of #555: GUI sem regra de negócio).

use anyhow::Result;

use crate::command::Command;
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
            Command::SaveChainPreset { name } => Ok(vec![Event::ChainPresetSaved { name }]),
            Command::DeleteChainPreset { name } => {
                if let Some(dir) = self.presets_path.borrow().as_ref() {
                    let path = preset_save_path(dir, &name);
                    if path.exists() {
                        std::fs::remove_file(&path).map_err(|e| {
                            anyhow::anyhow!(
                                "failed to remove preset file {path:?}: {e}"
                            )
                        })?;
                    }
                    // missing file is a silent no-op — same UX as the
                    // GUI's previous fs::remove_file path. Observers
                    // still get the event so they can refresh the
                    // picker.
                }
                Ok(vec![Event::ChainPresetDeleted { name }])
            }
            other => {
                unreachable!("handle_chain_preset received non-preset command: {other:?}")
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_preset_tests.rs"]
mod tests;
