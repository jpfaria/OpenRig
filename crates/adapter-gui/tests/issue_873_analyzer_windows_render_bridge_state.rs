//! #873 regression — the standalone Tuner and Spectrum windows render nothing
//! because the split publishes their state to a global they do not read.
//!
//! Reported by the owner with a screenshot: the Tuner window shows
//! "NO ACTIVE INPUTS" while `openrig://tuner` reports `running: true` and a
//! live reading (`G2 · 98.10 Hz`). The audio side is fine — the window is not
//! wired to the data.
//!
//! Root cause: commit c4ea00b9d moved the tuner/spectrum state onto
//! `AnalyzerBridge`, and `tuner_wiring` now publishes with
//! `AnalyzerBridge::get(&tuner_window).set_tuner_rows(..)`. But
//! `TunerWindow` still declares its own `in property <[TunerRow]> tuner-rows`
//! and feeds `TunerPanel` from THAT, so the published rows land in a global
//! nobody reads and the panel keeps seeing an empty list. Same for
//! `SpectrumWindow`. `MetronomeWindow` was done correctly (it reads
//! `MetronomeBridge`), which is the pattern these two must follow.
//!
//! Contract under test: a standalone analyzer window renders the state its
//! bridge carries — the same channel the Rust wiring publishes on.

use adapter_gui::{AnalyzerBridge, SpectrumRow, SpectrumWindow, TunerRow, TunerWindow};
use slint::{ComponentHandle, Global, ModelRc, VecModel};
use std::rc::Rc;

fn count_id(w: &impl ComponentHandle, id: &str) -> usize {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).count()
}

fn a_tuner_row() -> TunerRow {
    // The reading the owner's screenshot should have shown.
    TunerRow {
        label: "GUITARRA - DEFAULT - 2  ·  IN 1  ·  CH 1".into(),
        note: "G".into(),
        octave: 2,
        cents: 1.78,
        frequency: 98.1,
        active: true,
    }
}

fn a_spectrum_row() -> SpectrumRow {
    SpectrumRow {
        label: "GUITARRA - DEFAULT - 2".into(),
        levels: ModelRc::from(Rc::new(VecModel::from(vec![0.5f32; 63]))),
        peaks: ModelRc::from(Rc::new(VecModel::from(vec![0.6f32; 63]))),
        active: true,
    }
}

#[test]
fn tuner_window_renders_the_rows_its_bridge_carries() {
    i_slint_backend_testing::init_no_event_loop();
    let w = TunerWindow::new().unwrap();

    assert_eq!(
        count_id(&w, "TunerCard::root"),
        0,
        "a freshly opened tuner window has no cards"
    );

    AnalyzerBridge::get(&w)
        .set_tuner_rows(ModelRc::from(Rc::new(VecModel::from(vec![a_tuner_row()]))));

    assert_eq!(
        count_id(&w, "TunerCard::root"),
        1,
        "the tuner window must render the row published on its bridge — \
         this is the empty 'NO ACTIVE INPUTS' screen the owner reported"
    );
}

#[test]
fn tuner_window_power_state_follows_its_bridge() {
    i_slint_backend_testing::init_no_event_loop();
    let w = TunerWindow::new().unwrap();

    AnalyzerBridge::get(&w).set_tuner_enabled(true);

    assert!(
        w.get_tuner_enabled(),
        "the window's power state must follow the bridge the wiring writes to"
    );
}

#[test]
fn spectrum_window_renders_the_rows_its_bridge_carries() {
    i_slint_backend_testing::init_no_event_loop();
    let w = SpectrumWindow::new().unwrap();

    AnalyzerBridge::get(&w).set_spectrum_rows(ModelRc::from(Rc::new(VecModel::from(vec![
        a_spectrum_row(),
    ]))));

    assert_eq!(
        count_id(&w, "SpectrumCard::root"),
        1,
        "the spectrum window must render the row published on its bridge"
    );
}
