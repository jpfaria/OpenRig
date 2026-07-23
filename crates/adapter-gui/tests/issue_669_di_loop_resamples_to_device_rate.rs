//! Issue #669/#749 — a DI loop must play at the LIVE output device rate, not a
//! single hardcoded/global rate.
//!
//! Original #669 root cause: the loop was resampled once at LOAD time to the
//! dispatcher's `engine_sr`. #736 then clocked each runtime at its OWN device
//! rate, so on a multi-rate rig the single-rate buffer stretched on the
//! mismatched output — the owner's "está lento". #749 moves the resample to
//! ARM time, per output stream: `DiPcm` (the un-resampled source) is armed and
//! resampled to each runtime's rate.
//!
//! This test drives the real end-to-end path: the SAME 48 kHz source, ARMED on
//! a runtime at 48 kHz and on one at 44.1 kHz, must yield FEWER frames on the
//! armed 44.1 kHz runtime (resampled down). The loop crossfade is rate-relative
//! in both, so comparing the two lengths is robust to it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use application::command::Command;
use application::di_loader::DiLoopSource;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
use engine::runtime::{build_chain_runtime_state, RuntimeGraph, DEFAULT_ELASTIC_TARGET};
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::project::Project;

fn write_mono_wav(path: &Path, sr: u32, samples: &[f32]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("WavWriter::create");
    for &s in samples {
        w.write_sample(s).expect("write_sample");
    }
    w.finalize().expect("finalize");
}

fn make_project(chain_id: &str) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId(chain_id.to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    }))
}

/// A `ProjectRuntimeController` whose chain runtime AND reported sample rate
/// are `sr` — emulates a live stream opened at that device rate.
fn make_controller_at(chain_id: &ChainId, sr: u32) -> ProjectRuntimeController {
    let chain = Chain {
        id: chain_id.clone(),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    };
    let runtime_arc = Arc::new(
        build_chain_runtime_state(&chain, sr as f32, &[DEFAULT_ELASTIC_TARGET], &[])
            .expect("build_chain_runtime_state"),
    );
    let mut chains = HashMap::new();
    chains.insert((chain_id.clone(), 0), runtime_arc);
    ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, sr)
}

/// Load the same 48 kHz source, ARM it on a controller running at `device_sr`,
/// and return the armed runtime's loop frame count (its actual playback rate).
fn armed_loop_len_at(device_sr: u32, wav: &Path) -> usize {
    let chain_id = ChainId(format!("chain_669_{device_sr}"));
    let project = make_project(&chain_id.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let controller = RefCell::new(Some(make_controller_at(&chain_id, device_sr)));

    adapter_gui::di_loop_wiring::sync_engine_sr_from_runtime(&controller, &dispatcher);

    dispatcher
        .dispatch(Command::SetChainDiLoopSource {
            chain: chain_id.clone(),
            source: DiLoopSource::File(wav.to_path_buf()),
        })
        .expect("SetChainDiLoopSource must succeed");
    // #693: wait for the off-thread decode to land via the poll tick.
    {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while dispatcher.di_loop_for_chain(&chain_id).is_none()
            && std::time::Instant::now() < deadline
        {
            let _ = dispatcher.poll_async_results();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    // Arm it: the arm path resamples the source to the runtime's device rate.
    adapter_gui::di_loop_wiring::play_chain_di_loop(&controller, &dispatcher, &chain_id);

    // #771: the audible loop is the isolated pre-rendered playback; it parks
    // off-thread, so poll its length (rendered at the resolved output rate).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let len = controller
            .borrow()
            .as_ref()
            .expect("controller")
            .di_stream_loop_len(&chain_id);
        if let Some(len) = len {
            return len;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "a DI loop must be armed (render never parked)"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn di_loop_resamples_to_device_rate_not_hardcoded_48000() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("di_48k.wav");
    // 48 kHz source, 4800 frames (0.1 s).
    let samples: Vec<f32> = (0..4800).map(|i| ((i % 64) as f32 / 64.0) - 0.5).collect();
    write_mono_wav(&wav, 48_000, &samples);

    let len_48 = armed_loop_len_at(48_000, &wav);
    let len_44 = armed_loop_len_at(44_100, &wav);

    // Same source + rate-relative crossfade: the 48 kHz runtime replays 1:1,
    // the 44.1 kHz runtime must resample DOWN. If the loop is built at a single
    // global rate (bug #669/#749), both are identical and len_44 == len_48.
    assert!(
        len_44 < len_48,
        "REGRESSION #749: the DI loop did not resample to the 44.1 kHz device \
         rate — len@44100={len_44} is not < len@48000={len_48}; the armed loop \
         is stuck at a single global rate (plays in slow motion)."
    );
}
