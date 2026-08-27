//! #881 — re-pointing a routing block from the compact view.
//!
//! In the row, a processing block picks its MODEL where a routing block picks
//! its E/S — same widget, same slot — so the pick arrives on the model callback
//! and this module decides whether it is a binding change. What it must get
//! right: only routing blocks are handled (a processing block falls through to
//! the model path), an insert saves the binding, and a port saves the binding
//! plus that binding's first endpoint on its OWN side.

use super::dispatch_binding_pick;
use crate::state::ProjectSession;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InsertBlock, OutputBlock};
use project::chain::Chain;
use project::project::Project;
use std::cell::RefCell;
use std::rc::Rc;

const OTHER: &str = "io-other";

fn binding(id: &str, input: &str, output: &str) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.into(),
        inputs: vec![IoEndpoint {
            name: input.into(),
            device_id: DeviceId("dev-in".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: output.into(),
            device_id: DeviceId("dev-out".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}

fn block(id: &str, kind: AudioBlockKind) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind,
    }
}

/// input · gain · insert · output — the shape a compact row shows.
fn session() -> Rc<RefCell<Option<ProjectSession>>> {
    let chain = Chain {
        id: ChainId("chain:pick".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec!["io-1".into()],
        blocks: vec![
            block(
                "in",
                AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    io: "io-1".into(),
                    endpoint: "In 1".into(),
                }),
            ),
            block(
                "gain",
                AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "volume".into(),
                    params: Default::default(),
                }),
            ),
            block(
                "insert",
                AudioBlockKind::Insert(InsertBlock {
                    model: "external_loop".into(),
                    io: "io-1".into(),
                }),
            ),
            block(
                "out",
                AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    io: "io-1".into(),
                    endpoint: "Out 1".into(),
                }),
            ),
        ],
        di_output: None,
        loopers: vec![],
    };
    let session = ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![chain],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-903-pick-tests"),
    );
    *session.io_bindings.borrow_mut() = vec![
        binding("io-1", "In 1", "Out 1"),
        binding(OTHER, "Return A", "Send A"),
    ];
    Rc::new(RefCell::new(Some(session)))
}

fn chain_blocks(session: &Rc<RefCell<Option<ProjectSession>>>) -> Vec<AudioBlock> {
    session.borrow().as_ref().unwrap().project.borrow().chains[0]
        .blocks
        .clone()
}

#[test]
fn picking_on_a_processing_block_is_not_a_binding_change() {
    let session = session();

    assert!(
        !dispatch_binding_pick(&session, 0, 1, OTHER),
        "a gain block picks a MODEL — the caller must fall through to that path"
    );
}

#[test]
fn an_insert_saves_the_binding_it_was_pointed_at() {
    let session = session();

    assert!(dispatch_binding_pick(&session, 0, 2, OTHER));

    let AudioBlockKind::Insert(insert) = &chain_blocks(&session)[2].kind else {
        panic!("block 2 is the insert");
    };
    assert_eq!(insert.io, OTHER);
}

#[test]
fn an_input_port_takes_the_bindings_first_input_endpoint() {
    let session = session();

    assert!(dispatch_binding_pick(&session, 0, 0, OTHER));

    let AudioBlockKind::Input(input) = &chain_blocks(&session)[0].kind else {
        panic!("block 0 is the input port");
    };
    assert_eq!(input.io, OTHER);
    assert_eq!(
        input.endpoint, "Return A",
        "a port seeds the endpoint from its OWN side of the binding"
    );
}

#[test]
fn an_output_port_takes_the_bindings_first_output_endpoint() {
    let session = session();

    assert!(dispatch_binding_pick(&session, 0, 3, OTHER));

    let AudioBlockKind::Output(output) = &chain_blocks(&session)[3].kind else {
        panic!("block 3 is the output port");
    };
    assert_eq!(output.io, OTHER);
    assert_eq!(output.endpoint, "Send A");
}

#[test]
fn a_block_index_that_does_not_exist_is_not_handled() {
    let session = session();

    assert!(!dispatch_binding_pick(&session, 0, 9, OTHER));
    assert!(!dispatch_binding_pick(&session, 7, 0, OTHER));
}

#[test]
fn with_no_session_there_is_nothing_to_re_point() {
    let empty: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));

    assert!(!dispatch_binding_pick(&empty, 0, 0, OTHER));
}
