//! Issue #669 — RED-FIRST: a DI loop must be resampled to the LIVE device
//! sample rate, not the hardcoded 48000 default.
//!
//! Root cause: `LocalDispatcher.engine_sr` defaults to 48000 and its setter
//! `attach_engine_sr` was never called from the runtime wiring, so a DI loop
//! was always resampled to 48 kHz. On a 44.1 kHz stream the 48 kHz buffer
//! played ~0.92× — "slow motion".
//!
//! The fix wires the running controller's real sample rate into the
//! dispatcher (`sync_engine_sr_from_runtime`) whenever the runtime is
//! (re)built. This test drives that helper: the SAME 48 kHz source, loaded
//! once with the device at 48 kHz and once at 44.1 kHz, must yield FEWER
//! frames in the 44.1 kHz case (resampled down). The loop crossfade is
//! identical in both, so comparing the two lengths is robust to it.

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
    };
    let runtime_arc = Arc::new(
        build_chain_runtime_state(&chain, sr as f32, &[DEFAULT_ELASTIC_TARGET])
            .expect("build_chain_runtime_state"),
    );
    let mut chains = HashMap::new();
    chains.insert((chain_id.clone(), 0), runtime_arc);
    ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, sr)
}

/// Load the same 48 kHz source into a dispatcher whose engine sample rate was
/// synced from a controller running at `device_sr`; return the built loop's
/// frame count.
fn loaded_len_at(device_sr: u32, wav: &Path) -> usize {
    let chain_id = ChainId(format!("chain_669_{device_sr}"));
    let project = make_project(&chain_id.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let controller = RefCell::new(Some(make_controller_at(&chain_id, device_sr)));

    // The runtime wiring must push the controller's real rate into the
    // dispatcher BEFORE a DI loop is loaded.
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

    dispatcher
        .di_loop_for_chain(&chain_id)
        .expect("a DI loop must be loaded")
        .len()
}

#[test]
fn di_loop_resamples_to_device_rate_not_hardcoded_48000() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("di_48k.wav");
    // 48 kHz source, 4800 frames (0.1 s).
    let samples: Vec<f32> = (0..4800).map(|i| ((i % 64) as f32 / 64.0) - 0.5).collect();
    write_mono_wav(&wav, 48_000, &samples);

    let len_48 = loaded_len_at(48_000, &wav);
    let len_44 = loaded_len_at(44_100, &wav);

    // Same source + same crossfade: 48 kHz device replays 1:1, 44.1 kHz must
    // resample DOWN. If the dispatcher's engine_sr stays stuck at 48000
    // (bug #669), both loads are identical and len_44 == len_48.
    assert!(
        len_44 < len_48,
        "REGRESSION #669: DI loop did not resample to the 44.1 kHz device \
         rate — len@44100={len_44} is not < len@48000={len_48}; engine_sr is \
         stuck at the hardcoded 48000 (loop plays in slow motion)."
    );
}
