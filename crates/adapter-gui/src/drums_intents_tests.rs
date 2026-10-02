use std::cell::RefCell;
use std::rc::Rc;

use application::command::DrumsCommand;
use slint::{Global, SharedString};

use super::wire_drums_intents;
use crate::{DrumsBridge, DrumsWindow};

fn wired() -> (DrumsWindow, Rc<RefCell<Vec<String>>>) {
    i_slint_backend_testing::init_no_event_loop();
    let w = DrumsWindow::new().unwrap();
    let sent = Rc::new(RefCell::new(Vec::new()));
    let s = sent.clone();
    wire_drums_intents(
        &DrumsBridge::get(&w),
        Rc::new(move |cmd: DrumsCommand| s.borrow_mut().push(format!("{cmd:?}"))),
    );
    (w, sent)
}

#[test]
fn every_drums_control_dispatches_its_command() {
    let (w, sent) = wired();
    let bridge = DrumsBridge::get(&w);
    bridge.invoke_toggle_enabled(true);
    bridge.invoke_toggle_play();
    bridge.invoke_fill();
    bridge.invoke_set_bpm(132.0);
    bridge.invoke_set_volume(0.25);
    bridge.invoke_pick_kit(SharedString::from("black-pearl"));
    bridge.invoke_pick_groove(SharedString::from("rock-02"));
    bridge.invoke_pick_output(SharedString::from("main\u{1f}Out"));
    assert_eq!(
        *sent.borrow(),
        vec![
            format!("{:?}", DrumsCommand::SetDrumsEnabled { enabled: true }),
            format!("{:?}", DrumsCommand::ToggleDrums),
            format!("{:?}", DrumsCommand::TriggerDrumFill),
            format!("{:?}", DrumsCommand::SetDrumsBpm { bpm: 132.0 }),
            format!("{:?}", DrumsCommand::SetDrumsVolume { volume: 0.25 }),
            format!(
                "{:?}",
                DrumsCommand::SelectDrumKit {
                    kit: "black-pearl".into()
                }
            ),
            format!(
                "{:?}",
                DrumsCommand::SelectDrumGroove {
                    groove: "rock-02".into()
                }
            ),
            format!(
                "{:?}",
                DrumsCommand::SetDrumsOutput {
                    output_key: Some("main\u{1f}Out".into())
                }
            ),
        ]
    );
}
