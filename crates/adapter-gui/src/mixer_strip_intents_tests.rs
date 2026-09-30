//! #1007: a gesture on a mixer strip means the same mixer command wherever the
//! strip is hosted.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::MixerCommand;
use slint::Global;

use super::wire_strip_intents;
use crate::{CompactChainViewWindow, MixerBridge};

fn wired() -> (CompactChainViewWindow, Rc<RefCell<Vec<String>>>) {
    i_slint_backend_testing::init_no_event_loop();
    let w = CompactChainViewWindow::new().unwrap();
    let sent = Rc::new(RefCell::new(Vec::new()));
    let s = sent.clone();
    wire_strip_intents(
        &MixerBridge::get(&w),
        Rc::new(move |c: MixerCommand| s.borrow_mut().push(format!("{c:?}"))),
    );
    (w, sent)
}

#[test]
fn a_fader_move_sets_the_strip_gain() {
    let (w, sent) = wired();
    MixerBridge::get(&w).invoke_fader_moved("in:0".into(), 0.0);
    let got = sent.borrow();
    assert_eq!(got.len(), 1);
    assert!(got[0].starts_with("SetMixerFader"), "{got:?}");
    assert!(got[0].contains("\"in:0\""), "{got:?}");
}

#[test]
fn a_reset_puts_the_strip_back_at_unity() {
    let (w, sent) = wired();
    MixerBridge::get(&w).invoke_fader_reset("out:1".into());
    assert_eq!(
        *sent.borrow(),
        vec![format!(
            "{:?}",
            MixerCommand::SetMixerFader {
                strip: "out:1".into(),
                gain_db: 0.0
            }
        )]
    );
}

#[test]
fn mute_and_solo_toggle_the_strip() {
    let (w, sent) = wired();
    let bridge = MixerBridge::get(&w);
    bridge.invoke_mute_toggled("in:0".into());
    bridge.invoke_solo_toggled("in:0".into());
    assert_eq!(
        *sent.borrow(),
        vec![
            format!(
                "{:?}",
                MixerCommand::ToggleMixerMute {
                    strip: "in:0".into()
                }
            ),
            format!(
                "{:?}",
                MixerCommand::ToggleMixerSolo {
                    strip: "in:0".into()
                }
            ),
        ]
    );
}
