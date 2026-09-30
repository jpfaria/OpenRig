//! #1007 — HEADLESS proof that the compact chain view carries the chain's own
//! mixer strips and that they are operable: instantiate the real
//! `CompactChainViewWindow`, feed its `MixerBridge` the chain's strips and
//! press them. Same strip component and same bridge as the Mixer window, so
//! the wiring dispatches the same commands.

use std::cell::RefCell;
use std::rc::Rc;

use adapter_gui::{CompactChainViewWindow, MixerBridge, MixerStripRow};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, ModelRc, VecModel};

fn row(id: &str, is_input: bool) -> MixerStripRow {
    MixerStripRow {
        id: id.into(),
        name: id.into(),
        detail: if is_input { "IN 1" } else { "OUT 1,2" }.into(),
        is_input,
        position: 0.5,
        gain_label: "-12.0 dB".into(),
        muted: false,
        soloed: false,
        solo_silenced: false,
    }
}

fn window(inputs: Vec<MixerStripRow>, outputs: Vec<MixerStripRow>) -> CompactChainViewWindow {
    i_slint_backend_testing::init_no_event_loop();
    let w = CompactChainViewWindow::new().unwrap();
    w.window().set_size(slint::LogicalSize::new(900.0, 900.0));
    let bridge = MixerBridge::get(&w);
    bridge.set_inputs(ModelRc::new(VecModel::from(inputs)));
    bridge.set_outputs(ModelRc::new(VecModel::from(outputs)));
    w.show().unwrap();
    w
}

fn count(w: &CompactChainViewWindow, id: &str) -> usize {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).count()
}

fn press(w: &CompactChainViewWindow, id: &str, nth: usize) {
    let el = i_slint_backend_testing::ElementHandle::find_by_element_id(w, id)
        .nth(nth)
        .unwrap_or_else(|| panic!("{id} #{nth} not found"));
    let (pos, size) = (el.absolute_position(), el.size());
    let at = LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: at });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerReleased {
        position: at,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerExited);
}

#[test]
fn the_compact_view_shows_the_chain_inputs_and_outputs_together() {
    let w = window(vec![row("in:0@d", true)], vec![row("out:0,1@d", false)]);
    assert_eq!(count(&w, "MixerStripView::mute-ta"), 2);
}

#[test]
fn a_chain_without_strips_shows_no_mixer() {
    let w = window(vec![], vec![]);
    assert_eq!(count(&w, "MixerStripView::mute-ta"), 0);
}

#[test]
fn the_compact_strips_report_mute_and_solo_through_the_mixer_bridge() {
    let w = window(vec![row("in:0@d", true)], vec![row("out:0,1@d", false)]);
    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let bridge = MixerBridge::get(&w);
    let h = hits.clone();
    bridge.on_mute_toggled(move |id| h.borrow_mut().push(format!("mute {id}")));
    let h = hits.clone();
    bridge.on_solo_toggled(move |id| h.borrow_mut().push(format!("solo {id}")));
    press(&w, "MixerStripView::mute-ta", 1);
    press(&w, "MixerStripView::solo-ta", 0);
    assert_eq!(
        *hits.borrow(),
        vec!["mute out:0,1@d".to_string(), "solo in:0@d".to_string()]
    );
}
