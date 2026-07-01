//! Issue #669/#749 — changing the engine sample rate must flag an
//! ALREADY-LOADED DI loop so a playing chain is re-armed at the new rate.
//!
//! Real-world repro: a loop is loaded while the device runs at 48 kHz, then the
//! user switches to 44.1 kHz. The runtime rebuilds at 44100 but the DI buffer
//! kept its 48 kHz frames → plays at ≈0.92× ("slow motion"). #749 stores the
//! un-resampled `DiPcm` source and resamples at ARM time, so the store itself
//! is rate-independent; the rate-change contract is that `attach_engine_sr`
//! RETURNS every chain with a loaded source, so the wiring re-arms a playing
//! chain and its loop is rebuilt at the new device rate.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use application::command::Command;
use application::di_loader::DiLoopSource;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
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

#[test]
fn changing_engine_sr_flags_loaded_di_loop_for_rearm() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("di_48k.wav");
    // 48 kHz source, 4800 frames.
    let samples: Vec<f32> = (0..4800).map(|i| ((i % 64) as f32 / 64.0) - 0.5).collect();
    write_mono_wav(&wav, 48_000, &samples);

    let chain = ChainId("chain_669_rate".to_string());
    let project = make_project(&chain.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher.attach_engine_sr(48_000);
    dispatcher
        .dispatch(Command::SetChainDiLoopSource {
            chain: chain.clone(),
            source: DiLoopSource::File(wav.clone()),
        })
        .expect("SetChainDiLoopSource must succeed");
    // #693: the decode runs on its own task — wait for the completion
    // to land via poll_async_results before reading the loop back.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while dispatcher.di_loop_for_chain(&chain).is_none()
        && std::time::Instant::now() < deadline
    {
        let _ = dispatcher.poll_async_results();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        dispatcher.di_loop_for_chain(&chain).is_some(),
        "precondition: the source must be loaded"
    );

    // Device switches to 44.1 kHz. The store is rate-independent (DiPcm), so the
    // contract is that attach_engine_sr RETURNS the chain — the wiring uses that
    // to re-arm a playing chain, rebuilding the loop at the new device rate.
    let flagged = dispatcher.attach_engine_sr(44_100);
    assert!(
        flagged.contains(&chain),
        "REGRESSION #749: a rate change must flag the loaded chain for re-arm \
         (got {flagged:?}); without it a playing loop keeps its old-rate buffer \
         and drags in slow motion on the rebuilt 44.1 kHz runtime."
    );
    // The un-resampled source stays available for the re-arm.
    assert!(
        dispatcher.di_loop_for_chain(&chain).is_some(),
        "the source must remain loaded across a rate change"
    );
}
