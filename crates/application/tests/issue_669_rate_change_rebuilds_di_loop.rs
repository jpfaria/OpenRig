//! Issue #669 — RED-FIRST: changing the engine sample rate must rebuild an
//! ALREADY-LOADED DI loop at the new rate.
//!
//! Real-world repro (confirmed via stderr instrumentation): a loop is loaded
//! while the device runs at 48 kHz (`engine_sr=48000`), then the user switches
//! the device to 44.1 kHz. The runtime rebuilds at 44100 but the DI buffer
//! keeps its 48 kHz frames, so it plays at 44100/48000 ≈ 0.92× — "slow motion".
//! `#669`'s first cut only fixed FUTURE loads (engine_sr now tracks the rate);
//! this test pins that the EXISTING loaded loop is re-resampled on a rate change.

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
fn changing_engine_sr_rebuilds_loaded_di_loop_at_new_rate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("di_48k.wav");
    // 48 kHz source, 4800 frames.
    let samples: Vec<f32> = (0..4800).map(|i| ((i % 64) as f32 / 64.0) - 0.5).collect();
    write_mono_wav(&wav, 48_000, &samples);

    let chain = ChainId("chain_669_rate".to_string());
    let project = make_project(&chain.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Loaded while the device runs at 48 kHz: 48 kHz source at engine_sr 48000
    // is an identity (no resample).
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
    let len_48 = dispatcher
        .di_loop_for_chain(&chain)
        .expect("loop loaded")
        .len();

    // Device switches to 44.1 kHz. The already-loaded loop MUST be rebuilt at
    // the new rate (resampled down → fewer frames), or it plays in slow motion.
    dispatcher.attach_engine_sr(44_100);
    let len_44 = dispatcher
        .di_loop_for_chain(&chain)
        .expect("loop still present")
        .len();

    assert!(
        len_44 < len_48,
        "REGRESSION #669: changing engine_sr must rebuild the loaded DI loop \
         (len@44100={len_44} not < len@48000={len_48}); the loop kept its 48 kHz \
         buffer and plays in slow motion on the 44.1 kHz stream."
    );
}
