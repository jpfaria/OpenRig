//! Adapter-gui wiring for the per-chain virtual DI loop (issue #614).
//!
//! Two pure, testable pieces live here:
//!
//! ## A — `apply_di_loop_event`
//! Called by the Slint event-poll loop when `Event::ChainDiLoopEnabledChanged`
//! arrives. Receives the already-resolved `ChainRuntimeState` plus the
//! `Option<Arc<DiPcm>>` (the un-resampled source) fetched from the
//! dispatcher's ephemeral store, and `enabled`. The resample to the runtime's
//! rate happens here, off the audio thread.
//!
//! ## B — `di_loop_commands` / `DiLoopIntent`
//! Maps the four chain-tile DI control intents to `Vec<Command>`. No
//! `AppWindow`, no Slint — pure Rust. Task 7 (Slint) calls this from its
//! callbacks.

use std::sync::Arc;

use application::command::Command;
use application::di_loader::DiLoopSource;
use domain::ids::ChainId;
use engine::runtime::ChainRuntimeState;
use engine::DiPcm;

// ── A: runtime application helper ──────────────────────────────────────────

/// Apply a `ChainDiLoopEnabledChanged` event to a single `ChainRuntimeState`.
///
/// - `enabled = true`, `arc_opt = Some(di)` → arm the loop (audio picks it up).
/// - `enabled = true`, `arc_opt = None`    → no-op (source not loaded yet).
/// - `enabled = false`                     → always clear, `arc_opt` ignored.
///
/// This function is the only site that calls `rt.set_di_loop` adapter-side,
/// making it straightforward to unit-test without `AppWindow`.
///
/// #749: the stored source is un-resampled (`DiPcm`); resample to THIS
/// runtime's rate here so it plays at true speed on its output stream.
pub fn apply_di_loop_event(rt: &ChainRuntimeState, arc_opt: Option<Arc<DiPcm>>, enabled: bool) {
    if enabled {
        if let Some(pcm) = arc_opt {
            rt.set_di_loop(Some(Arc::new(pcm.to_loop_at(rt.sample_rate() as u32))));
        }
        // No arc → do nothing. The UI should only send enabled=true after a
        // source has been confirmed loaded (Task 7 enforces this).
    } else {
        rt.set_di_loop(None);
    }
}

// ── B: UI intent → command mapper ──────────────────────────────────────────

/// The four UI intents that the chain-tile DI control can express.
///
/// Constructed by Slint callbacks (Task 7) and passed to `di_loop_commands`.
pub enum DiLoopIntent {
    /// User picked a new source AND wants to start playing immediately.
    PlayWithNewSource { source: DiLoopSource },
    /// User pressed Play; source was already loaded by a prior interaction.
    Play,
    /// User pressed Stop.
    Stop,
    /// User picked a new source but did NOT press Play (pre-load / preview).
    SelectSource { source: DiLoopSource },
}

/// Map a chain-tile DI intent to the sequence of `Command`s to dispatch.
///
/// The returned `Vec` is always non-empty. Dispatch in order — the source
/// command must be dispatched before the enabled command so the dispatcher's
/// ephemeral store has the `Arc<DiLoop>` ready.
pub fn di_loop_commands(chain: ChainId, intent: DiLoopIntent) -> Vec<Command> {
    match intent {
        DiLoopIntent::PlayWithNewSource { source } => vec![
            Command::SetChainDiLoopSource {
                chain: chain.clone(),
                source,
            },
            Command::SetChainDiLoopEnabled {
                chain,
                enabled: true,
            },
        ],
        DiLoopIntent::Play => vec![Command::SetChainDiLoopEnabled {
            chain,
            enabled: true,
        }],
        DiLoopIntent::Stop => vec![Command::SetChainDiLoopEnabled {
            chain,
            enabled: false,
        }],
        DiLoopIntent::SelectSource { source } => vec![Command::SetChainDiLoopSource { chain, source }],
    }
}

// ── Event consumer (wires into the polling loop) ────────────────────────────

/// Handle `Event::ChainDiLoopEnabledChanged` adapter-side.
///
/// Resolves the chain's runtime(s) from `project_runtime`, fetches the stored
/// `Arc<DiLoop>` from `dispatcher` (if `enabled`), and calls
/// `apply_di_loop_event` on each runtime.
///
/// Mirrors the `Event::OutputMutedChanged` handler in `tuner_wiring.rs` but
/// scoped to a single chain (isolation invariant).
pub fn handle_chain_di_loop_enabled_changed(
    project_runtime: &std::cell::RefCell<Option<infra_cpal::ProjectRuntimeController>>,
    dispatcher: &application::local_dispatcher::LocalDispatcher,
    chain: &ChainId,
    enabled: bool,
) {
    let arc_opt: Option<Arc<DiPcm>> = if enabled {
        dispatcher.di_loop_for_chain(chain)
    } else {
        None
    };

    if let Some(rt) = project_runtime.borrow().as_ref() {
        rt.set_chain_di_loop(chain, if enabled { arc_opt } else { None });
    }
}

// ── Combined play / stop helpers (called from Slint callbacks) ──────────────
//
// Mirror the `wire_mute_inline` pattern in `tuner_wiring.rs` (lines 231-238):
// dispatch the command (so the event bus records the state change) and
// immediately apply the effect to the audio runtime — no polling loop needed.

/// Dispatch `SetChainDiLoopEnabled { enabled: true }` and apply the
/// `Arc<DiLoop>` stored in `dispatcher` to the chain's runtime.
///
/// If no source has been loaded yet (`di_loop_for_chain` returns `None`) the
/// apply is a no-op — the UI must guard the Play button until a source is
/// confirmed.  Dispatch still fires so the event bus stays in sync.
pub fn play_chain_di_loop(
    project_runtime: &std::cell::RefCell<Option<infra_cpal::ProjectRuntimeController>>,
    dispatcher: &application::local_dispatcher::LocalDispatcher,
    chain: &ChainId,
) {
    use application::dispatcher::CommandDispatcher;
    // #693: the DI decode runs on its own task — apply any completion
    // that already landed before arming, so play right after picking a
    // source uses the freshly decoded loop.
    let _ = dispatcher.poll_async_results();
    let _ = dispatcher.dispatch(application::command::Command::SetChainDiLoopEnabled {
        chain: chain.clone(),
        enabled: true,
    });
    handle_chain_di_loop_enabled_changed(project_runtime, dispatcher, chain, true);
}

/// Dispatch `SetChainDiLoopEnabled { enabled: false }` and clear the chain's
/// runtime immediately.
pub fn stop_chain_di_loop(
    project_runtime: &std::cell::RefCell<Option<infra_cpal::ProjectRuntimeController>>,
    dispatcher: &application::local_dispatcher::LocalDispatcher,
    chain: &ChainId,
) {
    use application::dispatcher::CommandDispatcher;
    let _ = dispatcher.dispatch(application::command::Command::SetChainDiLoopEnabled {
        chain: chain.clone(),
        enabled: false,
    });
    handle_chain_di_loop_enabled_changed(project_runtime, dispatcher, chain, false);
}

/// #669/#749: push the running controller's real device sample rate into the
/// dispatcher's `engine_sr` (the authoritative-rate fallback for consumers
/// that would otherwise assume 48000). No-op when no runtime is active.
///
/// Called from the runtime lifecycle whenever the controller is started or
/// re-synced (a sample-rate change rebuilds the runtime).
///
/// On an actual rate change, `attach_engine_sr` returns every chain with a
/// loaded DI source; we re-arm any chain whose loop is currently playing so
/// the arm path (`set_chain_di_loop`) rebuilds the loop at the runtime's NEW
/// rate — otherwise a loop that was *playing* when the device rate changed
/// drags in slow motion against its rebuilt runtime.
pub fn sync_engine_sr_from_runtime(
    project_runtime: &std::cell::RefCell<Option<infra_cpal::ProjectRuntimeController>>,
    dispatcher: &application::local_dispatcher::LocalDispatcher,
) {
    let rate = match project_runtime.borrow().as_ref() {
        Some(runtime) => runtime.sample_rate(),
        None => return,
    };
    let rebuilt = dispatcher.attach_engine_sr(rate);
    if rebuilt.is_empty() {
        return;
    }
    if let Some(runtime) = project_runtime.borrow().as_ref() {
        for chain in rebuilt {
            if runtime.chain_has_di_loop(&chain) {
                runtime.set_chain_di_loop(&chain, dispatcher.di_loop_for_chain(&chain));
            }
        }
    }
}
