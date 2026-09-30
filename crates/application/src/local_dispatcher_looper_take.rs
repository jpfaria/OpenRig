//! Responsibility: saves a looper's recorded mixdown as a take in the library.
//! #827 — the `SaveChainLooperTake` handler.
//!
//! The PCM comes from the same door the project save uses
//! (`RuntimeControl::export_chain_loops`, i.e. the controller's
//! `export_chain_looper` mixdown), so a take is byte-for-byte the audio the
//! looper holds — no second capture path. The wav is written here, on the
//! dispatching (control) thread; the audio thread never sees it.

use anyhow::Result;
use domain::ids::ChainId;

use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::looper_take_library::{save_take, take_file_name, TakeSaveError};

impl LocalDispatcher {
    /// Where takes are written: the attached override, else the OS default.
    fn looper_takes_dir(&self) -> std::path::PathBuf {
        self.looper_takes_path
            .borrow()
            .clone()
            .unwrap_or_else(infra_filesystem::default_looper_takes_path)
    }

    pub(crate) fn save_looper_take(
        &self,
        chain: ChainId,
        looper: u64,
        name: String,
    ) -> Result<Vec<Event>> {
        let looper = self.resolve_looper(&chain, looper)?;
        // A bad name is refused before the mixdown is fetched.
        take_file_name(&name)?;
        // No store hosted (the rig is stopped) and a looper with no material
        // are the same answer to the user: there is nothing to keep.
        let pcm = self
            .runtime_control()
            .zip(self.chain_def(&chain))
            .and_then(|(control, chain_def)| control.export_chain_loops(&chain_def))
            .and_then(|loops| loops.into_iter().find(|(uid, _)| *uid == looper))
            .map(|(_, pcm)| pcm)
            .ok_or(TakeSaveError::NothingRecorded)?;
        let path = save_take(
            &self.looper_takes_dir(),
            &name,
            pcm.samples(),
            pcm.sample_rate(),
        )?;
        Ok(vec![Event::ChainLooperTakeSaved {
            chain,
            looper,
            path,
        }])
    }
}

#[cfg(test)]
#[path = "local_dispatcher_looper_take_tests.rs"]
mod tests;
