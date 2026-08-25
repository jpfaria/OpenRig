//! #881 — removing (or adding) an insert must leave ONE set of streams behind.
//!
//! The owner's report: "tirei o insert e ficou um delay horrível; fechei e
//! reabri o projeto e normalizou — tem algo que não está matando o stream
//! antigo quando a gente altera, está trepando um por cima do outro."
//!
//! A doubled path is exactly what a leftover stream sounds like: the old and
//! the new runtime both process the same input and both write the output. These
//! open REAL streams (BlackHole) and count what the controller holds, because
//! the leak is in the controller's bookkeeping, not in the DSP.
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --lib issue_881_stream_lifecycle -- --nocapture
//! ```
#![cfg(target_os = "macos")]

use std::time::Duration;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

use crate::ProjectRuntimeController;

const RATE: u32 = 48_000;
const BUFFER: u32 = 128;
const LOOPBACK: &str = "BlackHole";

fn hw_enabled(name: &str) -> bool {
    if std::env::var("OPENRIG_HW_TESTS").is_ok() {
        return true;
    }
    eprintln!("[{name}] skipped — set OPENRIG_HW_TESTS=1 to run it (opens real streams)");
    false
}

fn settings(device_id: &str) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(device_id.into()),
        sample_rate: RATE,
        buffer_size_frames: BUFFER,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

fn loopback_device() -> Option<String> {
    crate::list_input_device_descriptors()
        .ok()?
        .into_iter()
        .find(|d| d.name.contains(LOOPBACK))
        .map(|d| d.id)
}

fn registry(device: &str) -> Vec<IoBinding> {
    let ep = |name: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![ep("in", 0)],
            outputs: vec![ep("out", 0)],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![ep("ret", 1)],
            outputs: vec![ep("snd", 1)],
        },
    ]
}

fn insert_block() -> AudioBlock {
    AudioBlock {
        id: BlockId("issue-881:insert".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "external_loop".into(),
            io: "fx".into(),
        }),
    }
}

fn chain_with(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("issue-881-lifecycle".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

/// (input streams, output streams) the controller is holding for the chain.
fn open_streams(controller: &ProjectRuntimeController, chain: &ChainId) -> (usize, usize) {
    controller
        .active_chains
        .get(chain)
        .map(|a| (a._input_streams.len(), a._output_streams.len()))
        .unwrap_or((0, 0))
}

/// Drive the controller the way `sync_live_chain_runtime` does for an enabled
/// chain: ask whether the I/O changed, drop the streams if it did, then
/// activate, polling until the build lands.
fn live_edit(controller: &mut ProjectRuntimeController, project: &Project, chain: &Chain) {
    let io_changed = controller
        .chain_io_changed(project, chain)
        .expect("io check");
    if io_changed {
        controller.remove_chain(&chain.id);
    }
    if !controller
        .schedule_chain_activation(project, chain)
        .expect("schedule")
        && !controller
            .request_offthread_rebuild_if_live(project, chain)
            .expect("rebuild")
    {
        controller.upsert_chain(project, chain).expect("upsert");
    }
    for _ in 0..80 {
        controller.poll_pending_rebuilds();
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn start(project: &Project, device: &str, chain_id: &ChainId) -> ProjectRuntimeController {
    let mut controller =
        ProjectRuntimeController::start_with_io_bindings(project, registry(device))
            .expect("start real streams");
    for _ in 0..100 {
        controller.poll_pending_rebuilds();
        if controller.stream_count(chain_id) > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    controller
}

/// Removing the insert must leave the chain with exactly the streams its new
/// topology needs — one input, one output. A leftover output stream is a second
/// writer on the same device: the doubled, delayed sound the owner heard.
#[test]
fn removing_the_insert_leaves_one_set_of_streams() {
    if !hw_enabled("removing_the_insert_leaves_one_set_of_streams") {
        return;
    }
    let Some(device) = loopback_device() else {
        eprintln!("skipped — needs the BlackHole loopback");
        return;
    };
    let chain_id = ChainId("issue-881-lifecycle".into());
    let with_insert = chain_with(vec![insert_block()]);
    let mut project = Project {
        name: Some("issue-881".into()),
        device_settings: vec![settings(&device)],
        chains: vec![with_insert.clone()],
        midi: None,
    };
    let mut controller = start(&project, &device, &chain_id);
    let bound = open_streams(&controller, &chain_id);
    eprintln!("[lifecycle] with insert: {bound:?} (in, out)");
    assert_eq!(
        bound,
        (1, 2),
        "a bound insert needs the tail output AND the send output"
    );

    let without = chain_with(vec![]);
    project.chains[0] = without.clone();
    live_edit(&mut controller, &project, &without);

    let after = open_streams(&controller, &chain_id);
    eprintln!("[lifecycle] insert removed: {after:?} (in, out)");
    assert_eq!(
        after,
        (1, 1),
        "#881: the send stream must be gone — a leftover writer on the same \
         device doubles the signal and is heard as a delay"
    );
}

/// The mirror: adding the insert to a live chain must not keep the old streams
/// alongside the new ones.
#[test]
fn adding_the_insert_does_not_stack_streams() {
    if !hw_enabled("adding_the_insert_does_not_stack_streams") {
        return;
    }
    let Some(device) = loopback_device() else {
        eprintln!("skipped — needs the BlackHole loopback");
        return;
    };
    let chain_id = ChainId("issue-881-lifecycle".into());
    let plain = chain_with(vec![]);
    let mut project = Project {
        name: Some("issue-881".into()),
        device_settings: vec![settings(&device)],
        chains: vec![plain.clone()],
        midi: None,
    };
    let mut controller = start(&project, &device, &chain_id);
    assert_eq!(open_streams(&controller, &chain_id), (1, 1));

    let with_insert = chain_with(vec![insert_block()]);
    project.chains[0] = with_insert.clone();
    live_edit(&mut controller, &project, &with_insert);

    let after = open_streams(&controller, &chain_id);
    eprintln!("[lifecycle] insert added: {after:?} (in, out)");
    assert_eq!(
        after,
        (1, 2),
        "#881: exactly the new topology — one input, tail + send outputs"
    );
}

/// A rebuild that was already in flight when the chain was removed must NOT
/// come back to life on the next poll: republishing a stale runtime into the
/// slots is the other way two graphs end up processing the same input.
#[test]
fn a_rebuild_in_flight_does_not_revive_a_removed_chain() {
    if !hw_enabled("a_rebuild_in_flight_does_not_revive_a_removed_chain") {
        return;
    }
    let Some(device) = loopback_device() else {
        eprintln!("skipped — needs the BlackHole loopback");
        return;
    };
    let chain_id = ChainId("issue-881-lifecycle".into());
    let with_insert = chain_with(vec![insert_block()]);
    let project = Project {
        name: Some("issue-881".into()),
        device_settings: vec![settings(&device)],
        chains: vec![with_insert.clone()],
        midi: None,
    };
    let mut controller = start(&project, &device, &chain_id);

    // Kick an off-thread rebuild and pull the rug: remove the chain before the
    // worker's result lands, then poll.
    let _ = controller.request_offthread_rebuild_if_live(&project, &with_insert);
    controller.remove_chain(&chain_id);
    for _ in 0..40 {
        controller.poll_pending_rebuilds();
        std::thread::sleep(Duration::from_millis(50));
    }

    assert_eq!(
        open_streams(&controller, &chain_id),
        (0, 0),
        "#881: a removed chain must stay removed — a late rebuild must not \
         resurrect its runtime behind the new one"
    );
    assert!(
        !controller
            .runtime_graph
            .chains
            .keys()
            .any(|(c, _)| c == &chain_id),
        "#881: no runtime may survive in the graph for a removed chain"
    );
}
