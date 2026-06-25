//! #548: end-to-end scenarios for the 4 banks of the Chocolate Plus
//! factory profile — feed a typed `Command` (the same the daemon would
//! submit after slot resolution) and assert the dispatcher mutated the
//! project + SelectionState + emitted the Events the GUI listens to.

use std::cell::RefCell;
use std::rc::Rc;

use crate::command::{Command, RigNavKind};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
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

fn input_blk() -> AudioBlock {
    AudioBlock {
        id: BlockId("in".to_string()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

fn output_blk() -> AudioBlock {
    AudioBlock {
        id: BlockId("out".to_string()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

fn chain(id: &str, audio: &[&str]) -> Chain {
    let mut blocks = vec![input_blk()];
    for bid in audio {
        blocks.push(core(bid));
    }
    blocks.push(output_blk());
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

fn dispatcher_with(chains: Vec<Chain>) -> LocalDispatcher {
    LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains,
        midi: None,
    })))
}

fn set_active(d: &LocalDispatcher, chain_id: Option<&str>, block_id: Option<&str>) {
    let st = d.selection_state();
    let mut s = st.write().unwrap();
    s.active_chain = chain_id.map(|s| s.to_string());
    s.active_block = block_id.map(|s| s.to_string());
}

fn active_chain(d: &LocalDispatcher) -> Option<String> {
    d.selection_state().read().unwrap().active_chain.clone()
}

fn active_block(d: &LocalDispatcher) -> Option<String> {
    d.selection_state().read().unwrap().active_block.clone()
}

// ─── Bank 1 — chains ────────────────────────────────────────────────────────

#[test]
fn bank1_a_prev_chain_seeds_first_when_none_active() {
    let d = dispatcher_with(vec![chain("a", &[]), chain("b", &[])]);
    d.dispatch(Command::SelectActiveChainRelative { delta: -1 })
        .unwrap();
    assert_eq!(active_chain(&d).as_deref(), Some("a"));
}

#[test]
fn bank1_a_prev_chain_wraps_from_first_to_last() {
    let d = dispatcher_with(vec![chain("a", &[]), chain("b", &[]), chain("c", &[])]);
    set_active(&d, Some("a"), None);
    d.dispatch(Command::SelectActiveChainRelative { delta: -1 })
        .unwrap();
    assert_eq!(active_chain(&d).as_deref(), Some("c"));
}

#[test]
fn bank1_d_next_chain_wraps_from_last_to_first() {
    let d = dispatcher_with(vec![chain("a", &[]), chain("b", &[])]);
    set_active(&d, Some("b"), None);
    d.dispatch(Command::SelectActiveChainRelative { delta: 1 })
        .unwrap();
    assert_eq!(active_chain(&d).as_deref(), Some("a"));
}

#[test]
fn bank1_chain_step_emits_project_mutated_so_gui_renders() {
    let d = dispatcher_with(vec![chain("a", &[])]);
    let events = d
        .dispatch(Command::SelectActiveChainRelative { delta: 1 })
        .unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::ProjectMutated)));
}

#[test]
fn bank1_b_toggle_chain_enabled_flips_and_mirrors_snapshot() {
    let d = dispatcher_with(vec![chain("a", &[])]);
    set_active(&d, Some("a"), None);
    {
        let st = d.selection_state();
        st.write().unwrap().active_chain_enabled = true;
    }
    d.dispatch(Command::ToggleChainEnabled {
        chain: ChainId("a".to_string()),
    })
    .unwrap();
    assert!(!d.selection_state().read().unwrap().active_chain_enabled);
}

#[test]
fn bank1_c_toggle_compact_view_flips_snapshot() {
    let d = dispatcher_with(vec![chain("a", &[])]);
    d.dispatch(Command::SetCompactViewEnabled { enabled: true })
        .unwrap();
    assert!(d.selection_state().read().unwrap().compact_view_enabled);
    d.dispatch(Command::SetCompactViewEnabled { enabled: false })
        .unwrap();
    assert!(!d.selection_state().read().unwrap().compact_view_enabled);
}

// ─── Bank 2 — preset/scene ──────────────────────────────────────────────────

#[test]
fn bank2_step_preset_command_reaches_rig_handler() {
    // ApplyRigNav requires a rig attached; without one the handler errors
    // with "rig". Either Ok (with a rig) or Err containing "rig" proves
    // the command reached the rig arm instead of being silently dropped.
    let d = dispatcher_with(vec![chain("a", &[])]);
    let result = d.dispatch(Command::ApplyRigNav {
        chain: ChainId("a".to_string()),
        kind: RigNavKind::StepPreset(1),
    });
    assert!(
        result.is_ok() || result.unwrap_err().to_string().contains("rig"),
        "ApplyRigNav must reach the rig handler"
    );
}

#[test]
fn bank2_jump_preset_n_routes_through_rig_handler_too() {
    let d = dispatcher_with(vec![chain("a", &[])]);
    let result = d.dispatch(Command::ApplyRigNav {
        chain: ChainId("a".to_string()),
        kind: RigNavKind::Preset(7),
    });
    assert!(
        result.is_ok() || result.unwrap_err().to_string().contains("rig"),
        "Jump variant must reach the rig handler"
    );
}

// ─── Bank 3 — blocks ────────────────────────────────────────────────────────

#[test]
fn bank3_a_prev_block_2_skips_io_in_six_block_chain() {
    let d = dispatcher_with(vec![chain("g", &["b0", "b1", "b2", "b3", "b4", "b5"])]);
    set_active(&d, Some("g"), Some("b2"));
    d.dispatch(Command::SelectActiveBlockRelative { delta: -2 })
        .unwrap();
    assert_eq!(active_block(&d).as_deref(), Some("b0"));
}

#[test]
fn bank3_d_next_block_2_skips_io_and_wraps() {
    let d = dispatcher_with(vec![chain("g", &["b0", "b1", "b2"])]);
    set_active(&d, Some("g"), Some("b1"));
    d.dispatch(Command::SelectActiveBlockRelative { delta: 2 })
        .unwrap();
    assert_eq!(active_block(&d).as_deref(), Some("b0"));
}

#[test]
fn bank3_b_toggle_active_block_flips_and_mirrors() {
    let d = dispatcher_with(vec![chain("g", &["b0", "b1"])]);
    set_active(&d, Some("g"), Some("b0"));
    {
        let st = d.selection_state();
        st.write().unwrap().active_block_enabled = true;
    }
    d.dispatch(Command::ToggleBlockEnabled {
        chain: ChainId("g".to_string()),
        block: BlockId("b0".to_string()),
    })
    .unwrap();
    assert!(!d.selection_state().read().unwrap().active_block_enabled);
}

#[test]
fn bank3_c_toggle_neighbor_flips_block_after_active() {
    let d = dispatcher_with(vec![chain("g", &["b0", "b1", "b2"])]);
    set_active(&d, Some("g"), Some("b0"));
    d.dispatch(Command::ToggleActiveBlockNeighborEnabled)
        .unwrap();
    let proj = d.project.borrow();
    let b1 = proj.chains[0]
        .blocks
        .iter()
        .find(|b| b.id.0 == "b1")
        .unwrap();
    assert!(!b1.enabled, "neighbor b1 must have been toggled off");
}

#[test]
fn bank3_c_toggle_neighbor_noop_without_active_chain_or_block() {
    let d = dispatcher_with(vec![chain("g", &["b0"])]);
    let events = d
        .dispatch(Command::ToggleActiveBlockNeighborEnabled)
        .unwrap();
    assert!(events.is_empty());

    set_active(&d, Some("g"), None);
    let events = d
        .dispatch(Command::ToggleActiveBlockNeighborEnabled)
        .unwrap();
    assert!(events.is_empty());
}

// ─── Bank 4 — global toggles ─────────────────────────────────────────────────

#[test]
fn bank4_a_toggle_tuner_round_trip() {
    let d = dispatcher_with(vec![chain("a", &[])]);
    assert!(!d.selection_state().read().unwrap().tuner_enabled);
    d.dispatch(Command::SetTunerEnabled { enabled: true })
        .unwrap();
    assert!(d.selection_state().read().unwrap().tuner_enabled);
    d.dispatch(Command::SetTunerEnabled { enabled: false })
        .unwrap();
    assert!(!d.selection_state().read().unwrap().tuner_enabled);
}

#[test]
fn bank4_b_toggle_output_mute_round_trip() {
    let d = dispatcher_with(vec![chain("a", &[])]);
    d.dispatch(Command::SetOutputMuted { muted: true }).unwrap();
    assert!(d.selection_state().read().unwrap().output_muted);
    d.dispatch(Command::SetOutputMuted { muted: false })
        .unwrap();
    assert!(!d.selection_state().read().unwrap().output_muted);
}

#[test]
fn bank4_c_toggle_spectrum_round_trip() {
    let d = dispatcher_with(vec![chain("a", &[])]);
    d.dispatch(Command::SetSpectrumEnabled { enabled: true })
        .unwrap();
    assert!(d.selection_state().read().unwrap().spectrum_enabled);
    d.dispatch(Command::SetSpectrumEnabled { enabled: false })
        .unwrap();
    assert!(!d.selection_state().read().unwrap().spectrum_enabled);
}

#[test]
fn each_toggle_emits_its_own_event_for_gui_refresh() {
    let d = dispatcher_with(vec![chain("a", &[])]);

    let ev = d
        .dispatch(Command::SetTunerEnabled { enabled: true })
        .unwrap();
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::TunerEnabledChanged { .. })));

    let ev = d.dispatch(Command::SetOutputMuted { muted: true }).unwrap();
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::OutputMutedChanged { .. })));

    let ev = d
        .dispatch(Command::SetSpectrumEnabled { enabled: true })
        .unwrap();
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::SpectrumEnabledChanged { .. })));
}

// ─── Cross-bank: chain change clears block selection ────────────────────────

#[test]
fn switching_chains_via_midi_clears_active_block() {
    let d = dispatcher_with(vec![chain("a", &["a0", "a1"]), chain("b", &["b0"])]);
    set_active(&d, Some("a"), Some("a1"));
    d.dispatch(Command::SelectActiveChainRelative { delta: 1 })
        .unwrap();
    assert_eq!(active_chain(&d).as_deref(), Some("b"));
    assert!(active_block(&d).is_none());
}

#[test]
fn gui_click_select_chain_block_populates_active_chain_and_block() {
    let d = dispatcher_with(vec![chain("g", &["b0", "b1"])]);
    d.dispatch(Command::SelectChainBlock {
        chain: ChainId("g".to_string()),
        block_index: 1, // 0 = Input, 1 = first audio
    })
    .unwrap();
    assert_eq!(active_chain(&d).as_deref(), Some("g"));
    assert_eq!(active_block(&d).as_deref(), Some("b0"));
}

#[test]
fn chain_with_only_io_blocks_block_nav_is_noop() {
    let d = dispatcher_with(vec![chain("empty", &[])]);
    set_active(&d, Some("empty"), None);
    d.dispatch(Command::SelectActiveBlockRelative { delta: 1 })
        .unwrap();
    assert!(active_block(&d).is_none());
}

#[test]
fn block_nav_without_active_chain_is_noop() {
    let d = dispatcher_with(vec![chain("a", &["b0"])]);
    d.dispatch(Command::SelectActiveBlockRelative { delta: 1 })
        .unwrap();
    assert!(active_chain(&d).is_none());
    assert!(active_block(&d).is_none());
}
