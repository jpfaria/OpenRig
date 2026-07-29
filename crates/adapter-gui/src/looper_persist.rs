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

use application::command::{Command, LooperCommand};
use application::looper_audio::{read_loop_wav, resample_loop, write_loop_wav};
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
            let pcm = controller.export_chain_looper(&chain, uid);
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
                .dispatch(Command::Looper(LooperCommand::SetChainLooperAudioFile {
                    chain: chain.clone(),
                    looper: uid,
                    file,
                }))
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
    // (chain, [(uid, input, output, saved wav)]). The chosen input/output are
    // seeded into the store so a restored loop records the same input and plays
    // to the same output it was saved with.
    #[allow(clippy::type_complexity)]
    let chains: Vec<(
        domain::ids::ChainId,
        Vec<(
            u64,
            Option<project::chain::EndpointRef>,
            Option<project::chain::EndpointRef>,
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
                        .map(|l| (l.uid, l.input.clone(), l.output.clone(), l.audio_file.clone()))
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

#[cfg(test)]
#[path = "looper_persist_tests.rs"]
mod tests;
