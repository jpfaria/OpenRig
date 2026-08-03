//! Adapter-gui wiring for the per-chain virtual DI loop (issue #614).
//!
//! #127: this module names no audio backend. Every DI control here is a
//! `Command` on the bus; the dispatcher applies the runtime effect through
//! `application::runtime_control::RuntimeControl`, whose GUI implementation
//! lives in `runtime_lifecycle` — the one module that owns the controller.
//!
//! ## A — `apply_di_loop_event`
//! Applies a `ChainDiLoopEnabledChanged` to a single already-resolved
//! `ChainRuntimeState`: the pre-#771 in-graph injection, kept for the runtime
//! tests that pin its semantics. The audible path is the isolated DI stream.
//!
//! ## B — `di_loop_commands` / `DiLoopIntent`
//! Maps the four chain-tile DI control intents to `Vec<Command>`. No
//! `AppWindow`, no Slint — pure Rust.
//!
//! ## C — play / stop / output select
//! What the Slint callbacks call: dispatch, and nothing else.

use std::sync::Arc;

use application::command::{ChainCommand, Command};
use application::di_loader::DiLoopSource;
use application::dispatcher::CommandDispatcher;
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
            Command::Chain(ChainCommand::SetChainDiLoopSource {
                chain: chain.clone(),
                source,
            }),
            Command::Chain(ChainCommand::SetChainDiLoopEnabled {
                chain,
                enabled: true,
            }),
        ],
        DiLoopIntent::Play => vec![Command::Chain(ChainCommand::SetChainDiLoopEnabled {
            chain,
            enabled: true,
        })],
        DiLoopIntent::Stop => vec![Command::Chain(ChainCommand::SetChainDiLoopEnabled {
            chain,
            enabled: false,
        })],
        DiLoopIntent::SelectSource { source } => {
            vec![Command::Chain(ChainCommand::SetChainDiLoopSource {
                chain,
                source,
            })]
        }
    }
}

// ── Play / stop / output helpers (called from Slint callbacks) ──────────────
//
// #127: these dispatch, and only dispatch. The runtime effect — arming the
// chain's isolated DI stream, disarming it, moving a playing loop to a newly
// picked output — is applied by the dispatcher through
// `application::runtime_control::RuntimeControl`, so a play asked for over
// MCP/gRPC/MIDI sounds exactly like the one asked for with the button.

/// User pressed ▶ on this chain's DI.
///
/// #808 is handled behind the door: arming an independent pipeline creates the
/// audio runtime if none is up, so the DI plays with no chain enabled.
pub fn play_chain_di_loop(dispatcher: &dyn CommandDispatcher, chain: &ChainId) {
    // #693: the DI decode runs on its own task — apply any completion that
    // already landed before arming, so play right after picking a source uses
    // the freshly decoded loop.
    let _ = dispatcher.poll_async_results();
    let _ = dispatcher.dispatch(Command::Chain(ChainCommand::SetChainDiLoopEnabled {
        chain: chain.clone(),
        enabled: true,
    }));
}

/// User pressed ■.
pub fn stop_chain_di_loop(dispatcher: &dyn CommandDispatcher, chain: &ChainId) {
    let _ = dispatcher.dispatch(Command::Chain(ChainCommand::SetChainDiLoopEnabled {
        chain: chain.clone(),
        enabled: false,
    }));
}

/// #771: the DI panel's OUTPUT select was picked. The index is a position in
/// the list the panel is showing, so it is resolved against the same options
/// builder here; the endpoint it names then travels as
/// `ChainCommand::SetChainDiLoopOutput`.
pub fn select_chain_di_output(
    dispatcher: &dyn CommandDispatcher,
    chain: &ChainId,
    registry: &[domain::io_binding::IoBinding],
    output_index: usize,
) {
    let Some(chain_def) = dispatcher.chain_snapshot(chain) else {
        return;
    };
    let options = crate::di_output_options::build_di_output_options(&chain_def, registry);
    let Some(option) = options.get(output_index) else {
        return;
    };
    let _ = dispatcher.dispatch(Command::Chain(ChainCommand::SetChainDiLoopOutput {
        chain: chain.clone(),
        output: option.di_ref.clone(),
    }));
}
