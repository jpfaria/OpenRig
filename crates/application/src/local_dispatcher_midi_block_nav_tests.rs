//! #548: MIDI block-nav handler must skip Input/Output blocks — the
//! user doesn't see them on the Chains screen, so a step that lands on
//! one looks like a no-op (the bug the user reported with the 6-block
//! chain + Bank 3 navigation on the Chocolate Plus).

use std::cell::RefCell;
use std::rc::Rc;

use crate::command::Command;
use crate::dispatcher::CommandDispatcher;
use crate::local_dispatcher::LocalDispatcher;
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

fn core(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "amp".to_string(),
            model: "test".to_string(),
            params: ParameterSet::default(),
        }),
    }
}

fn input_block() -> AudioBlock {
    AudioBlock {
        id: BlockId("input:0".to_string()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

fn output_block() -> AudioBlock {
    AudioBlock {
        id: BlockId("output:0".to_string()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

fn chain_with_io_and_blocks(id: &str, n_blocks: usize) -> Chain {
    let mut blocks = vec![input_block()];
    for i in 0..n_blocks {
        blocks.push(core(&format!("blk_{i}")));
    }
    blocks.push(output_block());
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
    }
}

fn project_with_chain(chain: Chain) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain],
        midi: None,
    }))
}

#[test]
fn first_block_nav_skips_input_block() {
    // Chain layout: [Input, blk_0, blk_1, blk_2, Output]
    // MIDI "+1 block" from no-selection MUST land on blk_0,
    // never on the Input wrapper.
    let chain = chain_with_io_and_blocks("guitar", 3);
    let dispatcher = LocalDispatcher::new(project_with_chain(chain));
    {
        let sel = dispatcher.selection_state();
        let mut s = sel.write().unwrap();
        s.active_chain = Some("guitar".to_string());
        s.active_block = None;
    }

    dispatcher
        .dispatch(Command::SelectActiveBlockRelative { delta: 1 })
        .unwrap();

    let sel = dispatcher.selection_state();
    let active = sel.read().unwrap().active_block.clone();
    assert_eq!(
        active.as_deref(),
        Some("blk_0"),
        "MIDI nav must skip InputBlock"
    );
}

#[test]
fn forward_step_after_last_audio_wraps_to_first_audio_not_output() {
    // Chain: [Input, blk_0, blk_1, blk_2, Output]
    // active_block = blk_2 (the LAST audio block).
    // +1 must wrap to blk_0, NEVER land on Output.
    let chain = chain_with_io_and_blocks("guitar", 3);
    let dispatcher = LocalDispatcher::new(project_with_chain(chain));
    {
        let sel = dispatcher.selection_state();
        let mut s = sel.write().unwrap();
        s.active_chain = Some("guitar".to_string());
        s.active_block = Some("blk_2".to_string());
    }

    dispatcher
        .dispatch(Command::SelectActiveBlockRelative { delta: 1 })
        .unwrap();

    let sel = dispatcher.selection_state();
    let active = sel.read().unwrap().active_block.clone();
    assert_eq!(
        active.as_deref(),
        Some("blk_0"),
        "wrap forward must skip OutputBlock"
    );
}

#[test]
fn backward_step_from_first_audio_wraps_to_last_audio_not_input() {
    // Chain: [Input, blk_0, blk_1, blk_2, Output]
    // active_block = blk_0. -1 must wrap to blk_2 (last audio),
    // NEVER land on Input.
    let chain = chain_with_io_and_blocks("guitar", 3);
    let dispatcher = LocalDispatcher::new(project_with_chain(chain));
    {
        let sel = dispatcher.selection_state();
        let mut s = sel.write().unwrap();
        s.active_chain = Some("guitar".to_string());
        s.active_block = Some("blk_0".to_string());
    }

    dispatcher
        .dispatch(Command::SelectActiveBlockRelative { delta: -1 })
        .unwrap();

    let sel = dispatcher.selection_state();
    let active = sel.read().unwrap().active_block.clone();
    assert_eq!(
        active.as_deref(),
        Some("blk_2"),
        "wrap backward must skip InputBlock"
    );
}

#[test]
fn two_step_navigation_skips_io_too() {
    // Chain: [Input, blk_0, blk_1, blk_2, blk_3, blk_4, blk_5, Output]
    // (user's reported case: 6 audio blocks)
    // active = blk_0, +2 must land on blk_2 (counting audio only,
    // not Input or Output).
    let chain = chain_with_io_and_blocks("guitar", 6);
    let dispatcher = LocalDispatcher::new(project_with_chain(chain));
    {
        let sel = dispatcher.selection_state();
        let mut s = sel.write().unwrap();
        s.active_chain = Some("guitar".to_string());
        s.active_block = Some("blk_0".to_string());
    }

    dispatcher
        .dispatch(Command::SelectActiveBlockRelative { delta: 2 })
        .unwrap();

    let active = dispatcher
        .selection_state()
        .read()
        .unwrap()
        .active_block
        .clone();
    assert_eq!(active.as_deref(), Some("blk_2"));
}

#[test]
fn no_navigable_blocks_is_noop() {
    // Chain with only Input + Output (no audio). MIDI nav must NOT panic
    // and active_block stays None.
    let chain = chain_with_io_and_blocks("empty", 0);
    let dispatcher = LocalDispatcher::new(project_with_chain(chain));
    {
        let sel = dispatcher.selection_state();
        let mut s = sel.write().unwrap();
        s.active_chain = Some("empty".to_string());
    }

    dispatcher
        .dispatch(Command::SelectActiveBlockRelative { delta: 1 })
        .unwrap();

    assert!(dispatcher
        .selection_state()
        .read()
        .unwrap()
        .active_block
        .is_none());
}
