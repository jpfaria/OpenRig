//! #127: the write-side counterpart to [`crate::live_source::LiveSource`].
//!
//! `LiveSource` lets the core READ what only a frontend's audio runtime
//! knows. `RuntimeControl` is the other direction: it lets a command handler
//! APPLY a state change to that runtime, so "every state change is born a
//! `Command`" holds for runtime control too. Before this, the GUI dispatched
//! the command AND then poked `ProjectRuntimeController` itself — which meant
//! MCP/gRPC dispatched the same command and nothing happened to the audio.
//!
//! The frontend that hosts the runtime implements this trait and hands the
//! dispatcher a shared instance via
//! [`crate::dispatcher::CommandDispatcher::attach_runtime_control`]. A
//! transport that owns no audio attaches nothing and keeps the default
//! no-ops: the command still succeeds and still reports its event, it just
//! has no runtime to touch.
//!
//! It is handed over as an `Rc` rather than a `Box` on purpose: a handler
//! clones the handle out of the dispatcher's `RefCell` and drops the borrow
//! BEFORE calling into the frontend. The frontend's sync sequence re-attaches
//! the control on its way out (it is the same funnel that installs it), so a
//! borrow held across the call would panic with `BorrowMutError`.
//!
//! **Not an audio-thread interface.** Every method is called on the
//! dispatching (UI/control) thread and must only flip control-plane state the
//! audio callback reads lock-free — no allocation, lock, syscall or I/O may
//! be pushed onto the audio thread from here.
//!
//! **Isolation (`CLAUDE.md` LAW).** A method that addresses a single stream
//! must carry that stream's identity; grouping runtimes by sample rate, or by
//! "all that match", is forbidden. `set_output_muted` is deliberately
//! rig-wide — the tuner silences the whole rig — and says so, rather than
//! pretending to be per-stream.

use anyhow::Result;

use domain::ids::{BlockId, ChainId};
use domain::io_binding::IoBinding;

/// Runtime state changes a command handler can apply to the frontend's audio
/// runtime.
///
/// Every method defaults to a no-op, meaning "this frontend hosts no audio
/// runtime". A frontend implements only the controls it can actually honour.
pub trait RuntimeControl {
    /// Silence (or un-silence) the rig's output.
    ///
    /// Rig-wide by design: this is the tuner's mute, which exists so the
    /// player can tune without the audience hearing it. It is NOT a
    /// per-stream control, and must not be repurposed as one — a per-stream
    /// mute would take the stream's identity as an argument.
    fn set_output_muted(&self, muted: bool) {
        let _ = muted;
    }

    /// Install the per-machine I/O binding registry the live runtime resolves
    /// its device endpoints against, so a rig that is ALREADY running picks up
    /// a binding edit instead of waiting for the next cold start.
    fn set_io_bindings(&self, bindings: Vec<IoBinding>) {
        let _ = bindings;
    }

    /// Apply one block's new enabled state to the chain that owns it, LIVE.
    ///
    /// This is issue #522's in-place fade toggle: the frontend flips the
    /// block's state on the running graph without re-resolving devices and
    /// without rebuilding the stream, so a stomp on a pedal never drops audio.
    /// An implementation must NOT turn this into a stream restart.
    ///
    /// **Isolation:** `chain` names the one runtime to touch. No other
    /// stream's runtime may be paused, rebuilt or otherwise observed — and
    /// never select runtimes by sample rate (`CLAUDE.md` LAW).
    fn set_block_enabled(&self, chain: &ChainId, block: &BlockId, enabled: bool) -> Result<()> {
        let _ = (chain, block, enabled);
        Ok(())
    }

    /// Bring ONE chain's live runtime back in step with the project — start it
    /// if it is enabled and nothing is running, pause it when it is disabled,
    /// drop it when it is gone.
    ///
    /// The sequence itself (device resolve, off-thread rebuild, activation
    /// scheduling) belongs to the frontend that owns the runtime; this is the
    /// door a `Command` knocks on so no UI callback has to reach for the
    /// controller directly.
    ///
    /// **Isolation:** exactly the chain named by `chain`. Syncing one chain
    /// must not touch, group with, or depend on any other stream's runtime.
    fn sync_chain(&self, chain: &ChainId) -> Result<()> {
        let _ = chain;
        Ok(())
    }
}
