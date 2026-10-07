//! #398 — the tuner says a string is in tune without words. Out of tune, the
//! arrow on the side of the note it has to move toward lights (flat: the
//! left one, pointing up the scale; sharp: the right one). In tune, both
//! light green and the scale's in-tune zone is marked, so the owner sees it
//! at a glance from the floor.

use adapter_gui::{AnalyzerBridge, TunerRow, TunerWindow};
use i_slint_backend_testing::ElementHandle;
use slint::{ComponentHandle, Global, ModelRc, VecModel};
use std::rc::Rc;

fn tuner(active: bool, cents: f32, in_tune: bool) -> TunerWindow {
    i_slint_backend_testing::init_no_event_loop();
    let w = TunerWindow::new().unwrap();
    AnalyzerBridge::get(&w).set_tuner_enabled(true);
    AnalyzerBridge::get(&w).set_tuner_rows(ModelRc::from(Rc::new(VecModel::from(vec![
        TunerRow {
            label: "GUITARRA 1 · IN 1".into(),
            note: if active { "E" } else { "—" }.into(),
            octave: if active { 2 } else { 0 },
            cents,
            frequency: if active { 82.4 } else { 0.0 },
            active,
            in_tune,
        },
    ]))));
    w.show().unwrap();
    w
}

fn lit(w: &TunerWindow, id: &str) -> bool {
    ElementHandle::find_by_element_id(w, id).count() == 1
}

#[test]
fn an_in_tune_string_lights_both_arrows() {
    let w = tuner(true, -1.0, true);
    assert!(lit(&w, "TunerCard::flat-lit"));
    assert!(lit(&w, "TunerCard::sharp-lit"));
}

#[test]
fn a_flat_string_lights_only_the_arrow_that_says_tune_up() {
    let w = tuner(true, -7.0, false);
    assert!(lit(&w, "TunerCard::flat-lit"));
    assert!(!lit(&w, "TunerCard::sharp-lit"));
}

#[test]
fn a_sharp_string_lights_only_the_arrow_that_says_tune_down() {
    let w = tuner(true, 18.0, false);
    assert!(!lit(&w, "TunerCard::flat-lit"));
    assert!(lit(&w, "TunerCard::sharp-lit"));
}

#[test]
fn a_silent_input_lights_no_arrow() {
    let w = tuner(false, 0.0, false);
    assert!(!lit(&w, "TunerCard::flat-lit"));
    assert!(!lit(&w, "TunerCard::sharp-lit"));
}

#[test]
fn the_scale_marks_the_in_tune_zone() {
    let w = tuner(true, 18.0, false);
    assert!(lit(&w, "TunerCard::zone"));
}
