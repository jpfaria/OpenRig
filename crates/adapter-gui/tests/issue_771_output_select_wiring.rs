//! #771 — picking an output in the DI panel must (1) persist it via
//! `Command::SetChainDiLoopOutput` and (2) move a PLAYING DI to the new
//! output (re-arm → re-render → park on the picked output's cell).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use application::command::Command;
use application::di_loader::DiLoopSource;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph, DEFAULT_ELASTIC_TARGET};
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, DiOutputRef};
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

fn registry() -> Vec<IoBinding> {
    let out = |name: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels,
    };
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![out("Main Out", vec![0, 1]), out("FX Out", vec![2, 3])],
    }]
}

fn test_chain(chain_id: &ChainId) -> Chain {
    Chain {
        id: chain_id.clone(),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
    }
}

fn make_project(chain_id: &ChainId) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![test_chain(chain_id)],
        midi: None,
    }))
}

fn make_controller(chain_id: &ChainId) -> ProjectRuntimeController {
    let chain = test_chain(chain_id);
    let reg = registry();
    let runtime_arc = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &reg)
            .expect("build_chain_runtime_state"),
    );
    let mut chains = HashMap::new();
    chains.insert((chain_id.clone(), 0), runtime_arc);
    let mut controller = ProjectRuntimeController::for_testing(RuntimeGraph { chains });
    controller.set_io_bindings(reg);
    controller
}

fn wait_for_output(
    controller: &RefCell<Option<ProjectRuntimeController>>,
    chain_id: &ChainId,
    expected: usize,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if controller
            .borrow()
            .as_ref()
            .and_then(|rt| rt.di_playback_active_output(chain_id))
            == Some(expected)
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn output_pick_persists_and_moves_a_playing_di() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("di.wav");
    write_mono_wav(&wav, 48_000, &[0.5f32; 256]);

    let chain_id = ChainId("chain_771_outsel".to_string());
    let project = make_project(&chain_id);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let controller = RefCell::new(Some(make_controller(&chain_id)));

    dispatcher
        .dispatch(Command::SetChainDiLoopSource {
            chain: chain_id.clone(),
            source: DiLoopSource::File(wav),
        })
        .expect("SetChainDiLoopSource must succeed");
    let deadline = Instant::now() + Duration::from_secs(2);
    while dispatcher.di_loop_for_chain(&chain_id).is_none() && Instant::now() < deadline {
        let _ = dispatcher.poll_async_results();
        std::thread::sleep(Duration::from_millis(10));
    }

    adapter_gui::di_loop_wiring::play_chain_di_loop(&controller, &dispatcher, &chain_id);
    assert!(
        wait_for_output(&controller, &chain_id, 0),
        "precondition: the DI plays on the main output before the pick"
    );

    // Pick the 2nd output (FX Out) — the panel callback path.
    adapter_gui::di_loop_wiring::select_chain_di_output(
        &controller,
        &dispatcher,
        &chain_id,
        &registry(),
        1,
    );

    assert_eq!(
        dispatcher
            .chain_snapshot(&chain_id)
            .expect("chain")
            .di_output,
        Some(DiOutputRef {
            binding_id: "io".into(),
            endpoint: "FX Out".into(),
        }),
        "#771: the pick must persist through Command::SetChainDiLoopOutput"
    );
    assert!(
        wait_for_output(&controller, &chain_id, 1),
        "#771: a PLAYING DI must move to the picked output (re-arm + re-park)"
    );
}
