//! Responsibility: applies a looper command to the controller's store.

use application::command::{LooperAction, LooperParam};
use application::looper_edit::LoopEdit;
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, EndpointRef};
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) type Runtime = Rc<RefCell<Option<ProjectRuntimeController>>>;

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

/// Mutate the store, then reconcile THIS chain's isolated playback streams.
///
/// The reconcile is part of every door, not an extra step a caller may forget:
/// the store is authoritative and updated on this same thread, so arming or
/// disarming right here is deterministic — there is no stale-status race to
/// wait a poll tick out for.
pub(crate) fn with_store(
    runtime: &Runtime,
    chain: &Chain,
    edit: impl FnOnce(&ProjectRuntimeController),
) {
    let borrow = runtime.borrow();
    let Some(controller) = borrow.as_ref() else {
        return;
    };
    edit(controller);
    controller.sync_looper_streams(chain);
}
