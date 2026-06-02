//! Adapter-gui wiring for the per-chain virtual DI loop (issue #614).
//!
//! Two pure, testable pieces live here:
//!
//! ## A — `apply_di_loop_event`
//! Called by the Slint event-poll loop when `Event::ChainDiLoopEnabledChanged`
//! arrives. Receives the already-resolved `ChainRuntimeState` plus the
//! `Option<Arc<DiLoop>>` fetched from the dispatcher's ephemeral store, and
//! `enabled`. Zero allocation, no locks on the caller side.
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
use engine::DiLoop;

// ── A: runtime application helper ──────────────────────────────────────────

/// Apply a `ChainDiLoopEnabledChanged` event to a single `ChainRuntimeState`.
///
/// - `enabled = true`, `arc_opt = Some(di)` → arm the loop (audio picks it up).
/// - `enabled = true`, `arc_opt = None`    → no-op (source not loaded yet).
/// - `enabled = false`                     → always clear, `arc_opt` ignored.
///
/// This function is the only site that calls `rt.set_di_loop` adapter-side,
/// making it straightforward to unit-test without `AppWindow`.
pub fn apply_di_loop_event(rt: &ChainRuntimeState, arc_opt: Option<Arc<DiLoop>>, enabled: bool) {
    if enabled {
        if let Some(di) = arc_opt {
            rt.set_di_loop(Some(di));
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
    let arc_opt: Option<Arc<DiLoop>> = if enabled {
        dispatcher.di_loop_for_chain(chain)
    } else {
        None
    };

    if let Some(rt) = project_runtime.borrow().as_ref() {
        rt.set_chain_di_loop(chain, if enabled { arc_opt } else { None });
    }
}
