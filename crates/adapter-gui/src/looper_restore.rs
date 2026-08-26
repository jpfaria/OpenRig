//! Responsibility: moves a project's recorded loops in out of the running store.

use application::looper_audio::{read_loop_wav, resample_loop};
use engine::LoopPcm;
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, EndpointRef};
use project::rig::RigProject;
use std::cell::RefCell;
use std::path::Path;
use std::sync::Arc;

use crate::looper_commands::Runtime;
use crate::state::ProjectSession;

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
