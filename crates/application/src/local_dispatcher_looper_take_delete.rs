//! Responsibility: deletes a saved take from the looper take library.
//! #1021 — the `DeleteLooperTake` handler.
//!
//! A chain whose DI has the take loaded keeps no reference to a file that is
//! gone: its isolated DI stream is disarmed and the source unloaded, chain by
//! chain. Every other chain — another take, a bundled loop, nothing — is left
//! exactly as it was. File I/O on the dispatching (control) thread only.

use std::path::Path;

use anyhow::Result;
use domain::ids::ChainId;

use crate::di_loader::DiLoopSource;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::looper_take_library::{delete_take, resolve_take};

impl LocalDispatcher {
    pub(crate) fn delete_looper_take(&self, name: String) -> Result<Vec<Event>> {
        let dir = self.looper_takes_dir();
        // Matched before the delete: a path only canonicalizes while it exists.
        let holders = self.chains_holding(&resolve_take(&dir, &name)?);
        let path = delete_take(&dir, &name)?;
        let mut events = Vec::new();
        for chain in holders {
            if let Some(control) = self.runtime_control() {
                control.disarm_di_stream(&chain);
            }
            self.di_loop_state.borrow_mut().remove(&chain);
            events.push(Event::ChainDiLoopEnabledChanged {
                chain: chain.clone(),
                enabled: false,
            });
            events.push(Event::ChainDiLoopSourceChanged { chain });
        }
        events.push(Event::LooperTakeDeleted { path });
        Ok(events)
    }

    /// The chains whose loaded DI source is the file at `take`.
    fn chains_holding(&self, take: &Path) -> Vec<ChainId> {
        let canonical = take.canonicalize().ok();
        self.di_loop_state
            .borrow()
            .iter()
            .filter(|(_, (source, _))| match source {
                DiLoopSource::File(path) => {
                    path == take || (canonical.is_some() && path.canonicalize().ok() == canonical)
                }
                DiLoopSource::Bundled(_) => false,
            })
            .map(|(chain, _)| chain.clone())
            .collect()
    }
}

#[cfg(test)]
#[path = "local_dispatcher_looper_take_delete_tests.rs"]
mod tests;
