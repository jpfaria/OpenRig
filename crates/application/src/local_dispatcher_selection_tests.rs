//! #22: the per-chain block-selection pair cursor lives behind the
//! dispatcher (not the GUI) so a footswitch moves it exactly like the
//! mouse. `SelectChainBlock` steps the cursor (wrapping); the footswitch
//! binds A=-2 / D=+2 ("anda de dois em dois"). `ToggleSelectedBlock`
//! flips one side of the selected pair (B=left, C=right).

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use crate::command::{ChainId, Command, PairSide};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

fn block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "amp".into(),
            model: "m".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn project_with_three_blocks() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("c1".into()),
            description: None,
            instrument: "guitar".into(),
            enabled: true,
            volume: 100.0,
            blocks: vec![block("b0"), block("b1"), block("b2")],
        }],
    }))
}

fn enabled(p: &Project) -> Vec<bool> {
    p.chains[0].blocks.iter().map(|b| b.enabled).collect()
}

#[test]
fn select_chain_block_steps_cursor_and_wraps() {
    let project = project_with_three_blocks();
    let d = LocalDispatcher::new(Rc::clone(&project));
    let c1 = ChainId("c1".into());

    // From the default cursor 0, +2 → 2.
    let ev = d
        .dispatch(Command::SelectChainBlock {
            chain: c1.clone(),
            delta: 2,
        })
        .expect("ok");
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::BlockSelectionChanged { left, .. } if *left == 2)),
        "cursor → 2: {ev:?}"
    );

    // +2 again wraps: (2+2) % 3 = 1.
    let ev = d
        .dispatch(Command::SelectChainBlock {
            chain: c1.clone(),
            delta: 2,
        })
        .expect("ok");
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::BlockSelectionChanged { left, .. } if *left == 1)));

    // -2 wraps backward: (1-2).rem_euclid(3) = 2.
    let ev = d
        .dispatch(Command::SelectChainBlock {
            chain: c1.clone(),
            delta: -2,
        })
        .expect("ok");
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::BlockSelectionChanged { left, .. } if *left == 2)));
}

#[test]
fn toggle_selected_block_flips_left_then_right_of_the_pair() {
    let project = project_with_three_blocks();
    let d = LocalDispatcher::new(Rc::clone(&project));
    let c1 = ChainId("c1".into());

    // Cursor at 0 (pair = b0,b1). B = left.
    d.dispatch(Command::ToggleSelectedBlock {
        chain: c1.clone(),
        side: PairSide::Left,
    })
    .expect("ok");
    assert_eq!(enabled(&project.borrow()), vec![false, true, true]);

    // C = right of the pair (b1).
    d.dispatch(Command::ToggleSelectedBlock {
        chain: c1.clone(),
        side: PairSide::Right,
    })
    .expect("ok");
    assert_eq!(enabled(&project.borrow()), vec![false, false, true]);
}
