//! `LocalDispatcher` read accessors (issue #792 split).
//!
//! Single responsibility: exposing dispatcher-owned state for reads
//! (selection, engine rate, DI-loop source, chain snapshot) plus the
//! immutable state-snapshot publish. No command handling, no wiring.

use std::sync::{Arc, RwLock};

use domain::ids::ChainId;
use engine::DiPcm;

use crate::di_loader::DiLoopSource;
use crate::local_dispatcher::LocalDispatcher;
use crate::selection_state::SelectionState;

impl LocalDispatcher {
    /// Shared handle to the GUI selection state. `Arc<RwLock<…>>` so
    /// the MIDI daemon thread can read the same state the GUI thread
    /// mutates; `Rc<RefCell<…>>` was tried first but `RefCell` is
    /// single-threaded and the daemon runs on its own midir-callback
    /// thread.
    pub fn selection_state(&self) -> Arc<RwLock<SelectionState>> {
        Arc::clone(&self.selection_state)
    }

    /// #693: clone the current state into an immutable snapshot for
    /// API-style reads (`crate::snapshot`). Called by
    /// `PublishingDispatcher` after every dispatch — the cost is one
    /// deep clone per command, paid on the writer thread, so readers
    /// never borrow the live `Rc` state.
    pub fn publish_state_snapshot(&self) {
        let project = self.project.borrow().clone();
        let rig = self.rig.borrow().as_ref().map(|rig| rig.borrow().clone());
        crate::snapshot::publish(crate::snapshot::StateSnapshot { project, rig });
    }

    /// The sample rate the live engine is currently running at, as last
    /// synced via [`Self::attach_engine_sr`]. Authoritative fallback for any
    /// consumer that would otherwise assume a fixed rate (issue #723).
    pub fn engine_sr(&self) -> u32 {
        *self.engine_sr.borrow()
    }

    /// #614: retrieve the pre-loaded DI loop arc for `chain`, if any.
    ///
    /// The adapter-gui wiring (Task 6) calls this from the
    /// `ChainDiLoopEnabledChanged { enabled: true }` event handler to
    /// forward the arc to the chain's audio runtime. Returns `None` when
    /// no source has been loaded for this chain yet.
    pub fn di_loop_for_chain(&self, chain: &ChainId) -> Option<Arc<DiPcm>> {
        self.di_loop_state
            .borrow()
            .get(chain)
            .map(|(_, arc)| Arc::clone(arc))
    }

    /// #717: a clone of the chain's current definition, so the runtime layer can
    /// build the dedicated DI runtime from a copy of the chain's graph without
    /// holding a borrow on the project.
    pub fn chain_snapshot(&self, chain: &ChainId) -> Option<project::chain::Chain> {
        self.project
            .borrow()
            .chains
            .iter()
            .find(|c| &c.id == chain)
            .cloned()
    }

    /// #661: retrieve WHICH source is currently loaded for `chain`, if any.
    ///
    /// Parity twin of [`Self::di_loop_for_chain`]: the GUI reads this back so
    /// the DI loop popup's ComboBox can highlight the active source when it is
    /// reopened (the popup is re-instantiated on each show, so the selection
    /// must be re-derived from dispatcher state rather than held in the view).
    /// Returns `None` when no source has been loaded for this chain yet.
    pub fn di_loop_source_for_chain(&self, chain: &ChainId) -> Option<DiLoopSource> {
        self.di_loop_state
            .borrow()
            .get(chain)
            .map(|(source, _)| source.clone())
    }
}
