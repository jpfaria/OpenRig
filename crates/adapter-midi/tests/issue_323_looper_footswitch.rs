//! Issue #323 — the looper is playable from a footswitch: each looper slot
//! resolves to a `SetChainLooperTransport` on the ACTIVE chain, addressing its
//! first looper (uid 0 = the sentinel the dispatcher resolves).

use adapter_midi::slots::{slot_to_command, IncomingMessage};
use application::command::{Command, LooperAction};
use application::SelectionState;

fn selection() -> SelectionState {
    SelectionState {
        active_chain: Some("chain:1".into()),
        ..Default::default()
    }
}

fn press() -> IncomingMessage {
    IncomingMessage::NoteOn {
        channel: 1,
        note: 60,
        velocity: 127,
    }
}

#[test]
fn every_looper_slot_maps_to_its_transport_action() {
    for (slot, action) in [
        ("looper_record", LooperAction::Record),
        ("looper_play_stop", LooperAction::PlayStop),
        ("looper_undo", LooperAction::Undo),
        ("looper_clear", LooperAction::Clear),
    ] {
        let cmd = slot_to_command(slot, &press(), &selection())
            .unwrap_or_else(|| panic!("{slot} must be a known slot"));
        match cmd {
            Command::SetChainLooperTransport {
                chain,
                looper,
                action: got,
            } => {
                assert_eq!(chain.0, "chain:1");
                assert_eq!(looper, 0, "a footswitch addresses the first looper");
                assert_eq!(got, action);
            }
            other => panic!("{slot} produced {other:?}"),
        }
    }
}

#[test]
fn looper_slots_need_an_active_chain() {
    let empty = SelectionState::default();
    assert!(slot_to_command("looper_record", &press(), &empty).is_none());
}
