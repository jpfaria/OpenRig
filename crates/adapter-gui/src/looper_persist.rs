//! #323 — persisting recorded loops with the project.
//!
//! A loop is audio, so it travels as a wav sidecar under `<project>.loops/`
//! and the chain only remembers the file name. Saving exports what the audio
//! thread holds; opening a project pushes it back into the fresh runtimes.
//!
//! Both directions run on the GUI thread: reading and writing files, and the
//! `export_looper` / `LoadLayer` calls that go with them, are control-side
//! work — the audio thread neither allocates nor touches the disk.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::looper_audio::{read_loop_wav, resample_loop, write_loop_wav};
use engine::LooperOp;
use infra_cpal::ProjectRuntimeController;

use crate::state::ProjectSession;

type Runtime = Rc<RefCell<Option<ProjectRuntimeController>>>;

/// Write every non-empty looper of every chain to disk and remember the file
/// name on the chain. Call right before the project is saved.
pub(crate) fn save_chain_loops(session: &ProjectSession, runtime: &Runtime, project_path: &Path) {
    let runtime_borrow = runtime.borrow();
    let Some(controller) = runtime_borrow.as_ref() else {
        return;
    };
    let rate = controller.sample_rate();
    let chains: Vec<(domain::ids::ChainId, Vec<u64>)> = {
        let project = session.project.borrow();
        project
            .chains
            .iter()
            .map(|c| (c.id.clone(), c.loopers.iter().map(|l| l.uid).collect()))
            .collect()
    };

    for (chain, loopers) in chains {
        for uid in loopers {
            let pcm = controller
                .runtimes_for_chain(&chain)
                .iter()
                .find_map(|rt| rt.export_looper(uid));
            let file = match pcm {
                Some(pcm) => match write_loop_wav(project_path, &chain, uid, &pcm, rate) {
                    Ok(name) => Some(name),
                    Err(err) => {
                        log::error!("saving loop {uid} of chain {}: {err}", chain.0);
                        continue;
                    }
                },
                // Nothing recorded: forget any stale pointer.
                None => None,
            };
            if let Err(err) = session
                .dispatcher
                .dispatch(Command::SetChainLooperAudioFile {
                    chain: chain.clone(),
                    looper: uid,
                    file,
                })
            {
                log::warn!("recording the loop file of {uid}: {err}");
            }
        }
    }
}

/// Claim a slot for every looper the project carries and install whatever
/// audio it saved. Call once the runtimes for a freshly-opened project exist.
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
    let chains: Vec<(domain::ids::ChainId, Vec<(u64, Option<String>)>)> = {
        let project = session.project.borrow();
        project
            .chains
            .iter()
            .map(|c| {
                (
                    c.id.clone(),
                    c.loopers
                        .iter()
                        .map(|l| (l.uid, l.audio_file.clone()))
                        .collect(),
                )
            })
            .collect()
    };

    for (chain, loopers) in chains {
        for (uid, file) in loopers {
            controller.push_chain_looper_op(&chain, |_| Some(LooperOp::Create { uid }));

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
            let frames = pcm.len() / 2;
            controller.push_chain_looper_op(&chain, |rt| {
                let max = rt.looper_max_frames();
                let len = frames.min(max);
                let mut buffer = vec![0.0f32; max * 2].into_boxed_slice();
                buffer[..len * 2].copy_from_slice(&pcm[..len * 2]);
                Some(LooperOp::LoadLayer {
                    uid,
                    buffer,
                    len_frames: len,
                })
            });
        }
    }
}

#[cfg(test)]
#[path = "looper_persist_tests.rs"]
mod tests;
