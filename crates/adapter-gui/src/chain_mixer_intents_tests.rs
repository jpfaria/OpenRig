//! #1007: a gesture on the compact view's chain faders becomes a command for
//! the chain the view shows, and nothing while it shows none.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, MixerCommand};
use domain::ids::ChainId;
use slint::Global;

use super::wire_chain_mixer_intents;
use crate::chain_fader_intent::ChainFaderIntent;
use crate::chain_mixer_rows::{DI_FADER_ID, MASTER_FADER_ID};
use crate::{ChainMixerBridge, CompactChainViewWindow};

fn wired(chain: Option<&str>) -> (CompactChainViewWindow, Rc<RefCell<Vec<String>>>) {
    i_slint_backend_testing::init_no_event_loop();
    let w = CompactChainViewWindow::new().unwrap();
    let sent = Rc::new(RefCell::new(Vec::new()));
    let s = sent.clone();
    let chain = chain.map(|c| ChainId(c.into()));
    wire_chain_mixer_intents(
        &ChainMixerBridge::get(&w),
        Rc::new(move || chain.clone()),
        Rc::new(move |i: ChainFaderIntent| {
            s.borrow_mut().push(match i {
                ChainFaderIntent::Dispatch(c) => format!("{c:?}"),
                ChainFaderIntent::MasterVolume(v) => format!("master {v}"),
            })
        }),
    );
    (w, sent)
}

fn mixer(c: MixerCommand) -> String {
    format!("{:?}", Command::Mixer(c))
}

#[test]
fn the_chain_fader_on_an_endpoint_sets_that_chain_gain() {
    let (w, sent) = wired(Some("c"));
    let bridge = ChainMixerBridge::get(&w);
    bridge.invoke_chain_fader_reset("out:1".into());
    bridge.invoke_chain_mute_toggled("out:1".into());
    assert_eq!(
        *sent.borrow(),
        vec![
            mixer(MixerCommand::SetChainMixerFader {
                chain: ChainId("c".into()),
                strip: "out:1".into(),
                gain_db: 0.0,
            }),
            mixer(MixerCommand::ToggleChainMixerMute {
                chain: ChainId("c".into()),
                strip: "out:1".into(),
            }),
        ]
    );
}

#[test]
fn a_chain_fader_move_carries_the_chain_and_the_strip() {
    let (w, sent) = wired(Some("c"));
    ChainMixerBridge::get(&w).invoke_chain_fader_moved("in:0".into(), 0.5);
    let got = sent.borrow();
    assert_eq!(got.len(), 1);
    assert!(got[0].contains("SetChainMixerFader"), "{got:?}");
    assert!(got[0].contains("\"in:0\""), "{got:?}");
}

#[test]
fn the_single_faders_go_through_the_chain_fader_intent() {
    let (w, sent) = wired(Some("c"));
    let bridge = ChainMixerBridge::get(&w);
    bridge.invoke_single_fader_reset(DI_FADER_ID.into());
    bridge.invoke_single_fader_reset(MASTER_FADER_ID.into());
    bridge.invoke_single_fader_moved(DI_FADER_ID.into(), 0.5);
    let got = sent.borrow();
    assert_eq!(got.len(), 3, "{got:?}");
    assert_eq!(
        got[0],
        mixer(MixerCommand::SetChainDiFader {
            chain: ChainId("c".into()),
            gain_db: 0.0,
        })
    );
    assert!(got[1].starts_with("master "), "{got:?}");
    assert!(got[2].contains("SetChainDiFader"), "{got:?}");
}

#[test]
fn no_chain_shown_means_no_command() {
    let (w, sent) = wired(None);
    let bridge = ChainMixerBridge::get(&w);
    bridge.invoke_chain_fader_moved("in:0".into(), 0.5);
    bridge.invoke_chain_mute_toggled("in:0".into());
    bridge.invoke_single_fader_moved(DI_FADER_ID.into(), 0.5);
    assert!(sent.borrow().is_empty());
}

#[test]
fn an_unknown_single_fader_is_ignored() {
    let (w, sent) = wired(Some("c"));
    ChainMixerBridge::get(&w).invoke_single_fader_reset("nope".into());
    assert!(sent.borrow().is_empty());
}
