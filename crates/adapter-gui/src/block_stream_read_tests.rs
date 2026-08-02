//! #127: the block editor's diagnostic stream, read through the seam.
//!
//! A utility block may publish a small table of already-reduced entries from
//! its worker thread; three surfaces render it (the detached editor window,
//! the inline drawer, the compact view) and all three used to hold
//! `ProjectRuntimeController` for the single call `poll_stream`.
//!
//! The reading is tri-shaped like the rest of `LiveSource`, and the panel
//! needs the distinction: nothing hosted is not the same as hosted-and-quiet.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;

use crate::gui_live_source::block_stream_live_source;

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
    }]
}

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn controller() -> Rc<RefCell<Option<ProjectRuntimeController>>> {
    let c = chain("chain:127:stream");
    let mut graph = HashMap::new();
    graph.insert(
        (c.id.clone(), 0usize),
        Arc::new(build_chain_runtime_state(&c, 48_000.0, &[1024], &registry()).unwrap()),
    );
    let mut controller = ProjectRuntimeController::for_testing(RuntimeGraph { chains: graph });
    controller.set_io_bindings(registry());
    Rc::new(RefCell::new(Some(controller)))
}

#[test]
fn a_hosted_runtime_answers_the_block_stream_read_even_when_the_block_is_quiet() {
    // The distinction the panel is built on: "hosted, this block publishes
    // nothing" must be an ANSWER (an empty table ⇒ the display goes inactive),
    // not the same silence as "there is no runtime at all".
    let runtime = controller();
    let live = block_stream_live_source(&runtime);

    let entries = live
        .block_stream(&BlockId("block:has:no:stream".into()))
        .expect("a hosted runtime always answers, even with nothing to report");
    assert!(
        entries.is_empty(),
        "a block that publishes no stream reports an empty table, never rows"
    );
}

#[test]
fn a_stopped_rig_hosts_no_block_stream() {
    // With nothing hosted the panel must keep whatever it was showing rather
    // than being told the block went quiet — the two look identical on screen
    // only if the seam collapses them, which is the bug this shape avoids.
    let runtime = Rc::new(RefCell::new(None));
    let live = block_stream_live_source(&runtime);

    assert!(
        live.block_stream(&BlockId("block:any".into())).is_none(),
        "no controller ⇒ this frontend hosts no block stream"
    );
}
