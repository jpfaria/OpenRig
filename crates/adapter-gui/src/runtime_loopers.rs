//! #127/#323: the looper doors' bodies — how a `LooperCommand` reaches the
//! controller's looper store, and how a project's loops leave and re-enter it.
//!
//! Looper state lives in the controller-owned store, NOT in the project: the
//! project only remembers that a looper exists and where its knobs are. So
//! every command has a runtime half, and before #127 that half was applied by
//! the GUI callback that had just dispatched (with the external-event drain
//! running a second, parallel copy). A looper driven over MCP or from a MIDI
//! footswitch mutated nothing.
//!
//! Everything here is reached through `application::runtime_control::RuntimeControl`
//! — a wiring module never calls it. Three rules hold across the file:
//!
//! * **isolation.** Every door is handed ONE chain and touches only that
//!   chain's store entries and its own isolated playback stream. Nothing is
//!   ever selected by sample rate or by "all that match" (`CLAUDE.md` LAW).
//! * **the reconcile belongs to the door.** Every mutation ends by syncing
//!   THAT chain's playback streams, so a closed loop sounds and a removed one
//!   goes quiet on the user's action, not on the next ~15 Hz meter tick.
//! * **#808: only a start may wake audio.** The wake itself is not here — it
//!   is the `ensure_runtime` precondition `GuiRuntimeControl` runs before
//!   [`create`] and before a Record / Play / PlayStop [`transport`], and
//!   before nothing else. A looper the user cannot record into is not a
//!   looper, and the panel arms REC only against a live store; stopping,
//!   clearing, undoing or turning a knob is never a reason to open a device.
//!   Everything in this file is a store mutation that no-ops when nothing is
//!   running.
//!
//! Nothing here runs on the audio thread: the store is mutated on the
//! dispatching thread and the callback reads it lock-free.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use application::command::{LooperAction, LooperParam};
use application::looper_audio::{read_loop_wav, resample_loop};
use application::looper_edit::LoopEdit;
use engine::LoopPcm;
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, EndpointRef};
use project::rig::RigProject;

use crate::state::ProjectSession;

/// The app-wide runtime handle, as every door here takes it.
type Runtime = Rc<RefCell<Option<ProjectRuntimeController>>>;

/// Claim this chain's store slot for a newly added looper.
pub fn create(runtime: &Runtime, chain: &Chain, looper: u64) {
    with_store(runtime, chain, |c| c.looper_create(&chain.id, looper));
}

/// Free the slot and silence the loop. The chain the door is handed no longer
/// lists this looper, so the reconcile disarms its stream in the same turn —
/// "delete stops the sound", instead of playing on until the next poll.
pub fn remove(runtime: &Runtime, chain: &Chain, looper: u64) {
    with_store(runtime, chain, |c| c.looper_remove(&chain.id, looper));
}

/// Apply one transport action to this looper.
///
/// `PlayStop` is resolved HERE, against the store's current state, so the one
/// on-screen button and a one-button footswitch behave identically.
pub fn transport(runtime: &Runtime, chain: &Chain, looper: u64, action: LooperAction) {
    with_store(runtime, chain, |c| match action {
        LooperAction::Record => c.looper_tap_record(&chain.id, looper),
        LooperAction::Stop => c.looper_stop(&chain.id, looper),
        LooperAction::Play => c.looper_play(&chain.id, looper),
        // One button, both actions — the store's current state decides.
        LooperAction::PlayStop => {
            if c.looper_is_playing(&chain.id, looper) {
                c.looper_stop(&chain.id, looper)
            } else {
                c.looper_play(&chain.id, looper)
            }
        }
        LooperAction::Undo => c.looper_undo(&chain.id, looper),
        LooperAction::Redo => c.looper_redo(&chain.id, looper),
        LooperAction::Clear => c.looper_clear(&chain.id, looper),
    });
}

/// #826: reshape a stopped loop and install the result, returning its new
/// length in frames. The store is the single gate: it refuses the edit while
/// the looper is not stopped, and `with_store` reconciles this chain's — and
/// only this chain's — playback stream so the shorter loop is what plays next.
pub fn apply_edit(
    runtime: &Runtime,
    chain: &Chain,
    looper: u64,
    edit: LoopEdit,
) -> anyhow::Result<usize> {
    let mut applied = 0;
    let mut refusal = None;
    with_store(runtime, chain, |c| {
        let (op, start, end) = edit.resolve();
        match c.looper_apply_edit(&chain.id, looper, op, start, end) {
            Ok(len) => applied = len,
            Err(err) => refusal = Some(err),
        }
    });
    match refusal {
        Some(err) => Err(anyhow::anyhow!("{err}")),
        None => Ok(applied),
    }
}

/// #826: step back one waveform edit on this loop.
pub fn undo_edit(runtime: &Runtime, chain: &Chain, looper: u64) {
    with_store(runtime, chain, |c| {
        c.looper_undo_edit(&chain.id, looper);
    });
}

/// #826: step forward one undone waveform edit on this loop.
pub fn redo_edit(runtime: &Runtime, chain: &Chain, looper: u64) {
    with_store(runtime, chain, |c| {
        c.looper_redo_edit(&chain.id, looper);
    });
}

/// #808: whether this transport action is a request to HEAR something, and may
/// therefore bring the audio runtime up. `Stop`, `Clear`, `Undo` and `Redo`
/// are not — silencing is never a reason to open a device.
pub fn transport_may_start_audio(action: LooperAction) -> bool {
    matches!(
        action,
        LooperAction::Record | LooperAction::Play | LooperAction::PlayStop
    )
}

/// Push a mix / decay / speed / reverse edit to the live loop. The store bumps
/// its content revision, so the reconcile re-renders a sounding loop through
/// the new setting without ever restarting the chain's stream.
pub fn set_param(runtime: &Runtime, chain: &Chain, looper: u64, param: LooperParam) {
    with_store(runtime, chain, |c| match param {
        LooperParam::Mix(v) => c.looper_set_mix(&chain.id, looper, v),
        LooperParam::Decay(v) => c.looper_set_decay(&chain.id, looper, v),
        LooperParam::Speed(s) => c.looper_set_speed(&chain.id, looper, s),
        LooperParam::Reverse(v) => c.looper_set_reverse(&chain.id, looper, v),
    });
}

/// Record from the chosen input from the next REC on (the store drops the
/// current record tap so it re-subscribes).
pub fn set_input(runtime: &Runtime, chain: &Chain, looper: u64, input: Option<EndpointRef>) {
    with_store(runtime, chain, |c| {
        c.looper_set_input(&chain.id, looper, input.clone())
    });
}

/// Play to the chosen output endpoint.
pub fn set_output(runtime: &Runtime, chain: &Chain, looper: u64, output: Option<EndpointRef>) {
    with_store(runtime, chain, |c| {
        c.looper_set_output(&chain.id, looper, output.clone())
    });
}

/// Hand every recorded loop of this chain over as an `Arc` HANDLE, for the
/// project save to write beside the project.
///
/// `None` ⇒ no controller, so no store is hosted: the caller must leave the
/// saved pointers alone (see the door's contract in
/// `application::runtime_control`). A looper missing from the returned list
/// holds no material.
pub fn export_chain_loops(runtime: &Runtime, chain: &Chain) -> Option<Vec<(u64, Arc<LoopPcm>)>> {
    let borrow = runtime.borrow();
    let controller = borrow.as_ref()?;
    let rate = controller.sample_rate();
    Some(
        chain
            .loopers
            .iter()
            .filter_map(|cfg| {
                let pcm = controller.export_chain_looper(&chain.id, cfg.uid)?;
                Some((cfg.uid, Arc::new(LoopPcm::new(pcm, rate))))
            })
            .collect(),
    )
}

/// #323: the restore as runtime creation asks for it — every path that brings a
/// controller up calls this, and a project that was never saved to disk has no
/// sidecar wavs to give back.
pub(crate) fn restore_project_loops(runtime: &Runtime, session: &ProjectSession) {
    let Some(project_path) = session.project_path.clone() else {
        return;
    };
    restore_chain_loops(session, runtime, &project_path);
}

/// #323: give freshly-created runtimes back the loopers the project carries,
/// with whatever audio each one saved.
///
/// NOT a `Command` and deliberately so: nobody ASKS for a restore. It is a
/// precondition of a controller existing — the same judgment `ensure_runtime`
/// gets — so it hangs off runtime creation, where every transport passes.
pub(crate) fn restore_chain_loops(
    session: &ProjectSession,
    runtime: &Runtime,
    project_path: &Path,
) {
    let runtime_borrow = runtime.borrow();
    let Some(controller) = runtime_borrow.as_ref() else {
        return;
    };
    let engine_rate = controller.sample_rate();
    // (chain, [(uid, input, output, saved wav)]). The chosen input/output are
    // seeded into the store so a restored loop records the same input and
    // plays to the same output it was saved with.
    #[allow(clippy::type_complexity)]
    let chains: Vec<(
        domain::ids::ChainId,
        Vec<(
            u64,
            Option<EndpointRef>,
            Option<EndpointRef>,
            Option<String>,
        )>,
    )> = {
        let project = session.project.borrow();
        project
            .chains
            .iter()
            .map(|c| {
                (
                    c.id.clone(),
                    c.loopers
                        .iter()
                        .map(|l| {
                            (
                                l.uid,
                                l.input.clone(),
                                l.output.clone(),
                                l.audio_file.clone(),
                            )
                        })
                        .collect(),
                )
            })
            .collect()
    };

    for (chain, loopers) in chains {
        for (uid, input, output, file) in loopers {
            controller.looper_create(&chain, uid);
            controller.looper_set_input(&chain, uid, input);
            controller.looper_set_output(&chain, uid, output);

            let Some(file) = file else { continue };
            let (pcm, file_rate) = match read_loop_wav(project_path, &file) {
                Ok(loaded) => loaded,
                Err(err) => {
                    // A missing sidecar must never block opening a project.
                    log::warn!("loop {file} of chain {} not restored: {err}", chain.0);
                    continue;
                }
            };
            // A loop recorded at 44.1 kHz would play 9 % fast on a 48 kHz
            // stream (#669) — resample to the rate the streams actually run.
            let pcm = resample_loop(&pcm, file_rate, engine_rate);
            controller.looper_load(&chain, uid, &pcm);
        }
    }
}

/// #323 phase 2: resolve each looper's LINKED preset into the effect blocks it
/// plays through and push them into the store, so a Playing loop renders
/// through its fixed tone rather than the chain's current preset. A looper with
/// no link (or a non-rig session) clears its override, falling back to the
/// chain's blocks.
///
/// Called on the meter tick, just before the isolated playback streams are
/// reconciled; the store bumps its re-arm generation only on a real change, so
/// a steady loop never respawns.
pub(crate) fn sync_playback_presets(
    controller: &ProjectRuntimeController,
    chain: &Chain,
    rig: Option<&RefCell<RigProject>>,
) {
    // The rig input name is the chain id minus the `rig:` projection prefix; a
    // non-rig chain has neither a rig nor a linked preset.
    let input_name = chain.id.0.strip_prefix("rig:");
    for cfg in &chain.loopers {
        let blocks = match (cfg.preset.as_deref(), input_name, rig) {
            (Some(preset_id), Some(input), Some(rig)) => {
                engine::rig_runtime::looper_playback_blocks(&rig.borrow(), input, preset_id)
            }
            _ => None,
        };
        controller.looper_set_playback_blocks(&chain.id, cfg.uid, blocks);
    }
}

/// #127/#323: the meter tick's per-chain reconcile, behind
/// `RuntimeControl::reconcile_chain_loopers`.
///
/// Four steps, in this order and for THIS chain only: make sure every looper
/// the project carries has a slot (added via any transport, or loaded from
/// disk), feed whatever is recording from its own input tap, resolve each
/// loop's LINKED preset into the blocks it plays through, and only then
/// reconcile the isolated playback streams — the presets must be in place
/// before the streams are armed, or a Playing loop renders through the chain's
/// current preset instead of its own.
///
/// Layer buffers the audio thread finished with come back on this path;
/// dropping them is forbidden on the audio thread (invariant #8).
pub(crate) fn reconcile_chain_loopers(
    runtime: &Runtime,
    chain: &Chain,
    rig: Option<&RefCell<RigProject>>,
) {
    let borrow = runtime.borrow();
    let Some(controller) = borrow.as_ref() else {
        return;
    };
    controller.sync_looper_slots(chain);
    controller.drain_looper_recording(chain);
    sync_playback_presets(controller, chain, rig);
    controller.sync_looper_streams(chain);
}

/// Mutate the store, then reconcile THIS chain's isolated playback streams.
///
/// The reconcile is part of every door, not an extra step a caller may forget:
/// the store is authoritative and updated on this same thread, so arming or
/// disarming right here is deterministic — there is no stale-status race to
/// wait a poll tick out for.
fn with_store(runtime: &Runtime, chain: &Chain, edit: impl FnOnce(&ProjectRuntimeController)) {
    let borrow = runtime.borrow();
    let Some(controller) = borrow.as_ref() else {
        return;
    };
    edit(controller);
    controller.sync_looper_streams(chain);
}

#[cfg(test)]
#[path = "runtime_loopers_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "runtime_loopers_808_tests.rs"]
mod tests_808;

#[cfg(test)]
#[path = "runtime_loopers_826_tests.rs"]
mod tests_826;
