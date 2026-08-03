//! `LocalDispatcher` read accessors (issue #792 split).
//!
//! Single responsibility: exposing dispatcher-owned state for reads
//! (selection, engine rate, DI-loop source, chain snapshot) plus the
//! immutable state-snapshot publish. No command handling, no wiring.
//!
//! #127: these read accessors are `pub(crate)` — the crate's public surface
//! for them is `CommandDispatcher` (see `local_dispatcher_trait.rs`), which
//! delegates one line into each. They stay here (not inlined into the trait
//! impl) because Rust allows exactly one `impl CommandDispatcher for
//! LocalDispatcher` block per crate (E0119 otherwise), and concentrating
//! every trait method's full body there would blow past this file's line
//! cap; keeping the bodies in their existing single-responsibility modules
//! and delegating from the trait impl satisfies both constraints.

use std::sync::{Arc, RwLock};

use domain::ids::ChainId;
use engine::DiPcm;

use crate::di_loader::DiLoopSource;
use crate::local_dispatcher::LocalDispatcher;
use crate::selection_state::SelectionState;
use crate::tone_doctor_report::ToneRun;

impl LocalDispatcher {
    /// Shared handle to the GUI selection state. `Arc<RwLock<…>>` so
    /// the MIDI daemon thread can read the same state the GUI thread
    /// mutates; `Rc<RefCell<…>>` was tried first but `RefCell` is
    /// single-threaded and the daemon runs on its own midir-callback
    /// thread.
    pub(crate) fn selection_state(&self) -> Arc<RwLock<SelectionState>> {
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
    pub(crate) fn engine_sr(&self) -> u32 {
        *self.engine_sr.borrow()
    }

    /// #614: retrieve the pre-loaded DI loop arc for `chain`, if any.
    ///
    /// The adapter-gui wiring (Task 6) calls this from the
    /// `ChainDiLoopEnabledChanged { enabled: true }` event handler to
    /// forward the arc to the chain's audio runtime. Returns `None` when
    /// no source has been loaded for this chain yet.
    pub(crate) fn di_loop_for_chain(&self, chain: &ChainId) -> Option<Arc<DiPcm>> {
        self.di_loop_state
            .borrow()
            .get(chain)
            .map(|(_, arc)| Arc::clone(arc))
    }

    /// #717: a clone of the chain's current definition, so the runtime layer can
    /// build the dedicated DI runtime from a copy of the chain's graph without
    /// holding a borrow on the project.
    pub(crate) fn chain_snapshot(&self, chain: &ChainId) -> Option<project::chain::Chain> {
        self.project
            .borrow()
            .chains
            .iter()
            .find(|c| &c.id == chain)
            .cloned()
    }

    /// #791: the chain's last Tone Doctor run as
    /// `{"state": …, "error": …, "tone": …}`.
    ///
    /// `state` is `idle` (never asked), `running`, `ok` or `failed` — a reader
    /// with no access to the events (MCP, gRPC) learns from this alone whether
    /// the verdict it is holding is fresh, still coming, or never arrived.
    /// Serving it from the dispatcher rather than from whichever frontend ran
    /// the diagnosis is what makes MCP see exactly what the GUI panel shows,
    /// instead of paying for a second render of its own.
    pub(crate) fn tone_report_json(&self, chain: &ChainId) -> String {
        let run = self
            .tone_doctor_runs
            .borrow()
            .get(chain)
            .cloned()
            .unwrap_or_else(ToneRun::idle);
        serde_json::to_string(&run)
            .unwrap_or_else(|e| format!("{{\"state\":\"failed\",\"error\":\"{e}\",\"tone\":null}}"))
    }

    /// #661: retrieve WHICH source is currently loaded for `chain`, if any.
    ///
    /// Parity twin of [`Self::di_loop_for_chain`]: the GUI reads this back so
    /// the DI loop popup's ComboBox can highlight the active source when it is
    /// reopened (the popup is re-instantiated on each show, so the selection
    /// must be re-derived from dispatcher state rather than held in the view).
    /// Returns `None` when no source has been loaded for this chain yet.
    pub(crate) fn di_loop_source_for_chain(&self, chain: &ChainId) -> Option<DiLoopSource> {
        self.di_loop_state
            .borrow()
            .get(chain)
            .map(|(source, _)| source.clone())
    }
}
