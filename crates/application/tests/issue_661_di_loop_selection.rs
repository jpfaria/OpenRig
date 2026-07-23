//! Issue #661 — RED-FIRST test for `LocalDispatcher::di_loop_source_for_chain`.
//!
//! The DI loop popup must reflect the currently selected source when reopened.
//! The selected source already lives in the dispatcher's ephemeral
//! `di_loop_state`, but only the decoded arc was exposed
//! (`di_loop_for_chain`). The GUI also needs to read back WHICH source is
//! loaded so the ComboBox can highlight it. This getter is the parity twin
//! of `di_loop_for_chain`.

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

/// Write a minimal valid mono PCM-float WAV at the given sample rate.
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

/// Before any source is set, the getter returns `None`.
#[test]
fn di_loop_source_for_chain_is_none_before_selection() {
    let project = make_project("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    assert!(
        dispatcher
            .di_loop_source_for_chain(&ChainId("chain_0".to_string()))
            .is_none(),
        "no source selected yet ⇒ None"
    );
}

/// After `SetChainDiLoopSource`, the getter returns the source that was loaded.
#[test]
fn di_loop_source_for_chain_returns_selected_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("di.wav");
    write_mono_wav(&wav, 48_000, &[0.5f32; 64]);

    let project = make_project("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::SetChainDiLoopSource {
            chain: ChainId("chain_0".to_string()),
            source: DiLoopSource::File(wav.clone()),
        })
        .expect("source must load");

    // #693: the decode runs on its own task — wait for the completion
    // to be installed via poll_async_results (the frontend tick's job).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while dispatcher
        .di_loop_source_for_chain(&ChainId("chain_0".to_string()))
        .is_none()
        && std::time::Instant::now() < deadline
    {
        let _ = dispatcher.poll_async_results();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let got = dispatcher.di_loop_source_for_chain(&ChainId("chain_0".to_string()));
    assert!(
        matches!(got, Some(DiLoopSource::File(ref p)) if *p == wav),
        "expected Some(File({wav:?})), got {got:?}"
    );
}
