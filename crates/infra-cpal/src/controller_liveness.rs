//! Responsibility: says whether the runtime is actually sounding.
//! line cap, like `controller_chain_activation` and `controller_block_toggle`.
//!
//! This is the predicate the chain-teardown doors act on: `adapter-gui`'s
//! `sync_live_chain_runtime` and `remove_live_chain_runtime` DROP the whole
//! controller once it answers `false`. So it has to count everything that can
//! make a sound, not just the chains.

use crate::ProjectRuntimeController;

impl ProjectRuntimeController {
    /// Whether anything this controller owns is audible or on its way to being
    /// audible.
    ///
    /// An enabled chain is the obvious case. The rest are the pipelines that
    /// stand on their own (invariant #4), each of which the teardown doors
    /// would otherwise silence by dropping the controller under them:
    ///
    /// * #672 — a cold activation still building has no stream yet, but the
    ///   user already asked to hear it;
    /// * #808 — an armed DI owns its stream and plays with no chain enabled;
    /// * #127 — so does the metronome. Start the click with no chain enabled,
    ///   then enable a chain and disable it: the teardown door asks this
    ///   question, and with the click's stream uncounted the answer was "no".
    ///   The controller went, the click died, and with the runtime gone the
    ///   lamp timer early-returned, leaving POWER lit over silence.
    pub fn is_running(&self) -> bool {
        !self.active_chains.is_empty()
            || !self.pending_activations.is_empty()
            || !self.di_streams.borrow().is_empty()
            || self.metronome_active()
    }
}
