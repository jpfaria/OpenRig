//! #1007 — HEADLESS proof that a mixer fader can be DRAGGED, in the Mixer
//! window panel and in the compact chain view strip.
//!
//! User report: dragging a fader up or down feels stuck. A drag here is a
//! real press, several pointer moves and a release on the fader. The cap has to
//! follow the pointer on every move (without waiting for the app to redraw the
//! strips), the drag has to survive the app replacing the strip model while
//! the button is held (the open mixer re-reads the dispatcher every 100 ms),
//! and the last reported position has to be the drop position.

use std::cell::RefCell;
use std::rc::Rc;

use adapter_gui::{
    set_mixer_rows, CompactChainViewWindow, MixerBridge, MixerHarness, MixerStripRow,
};
use i_slint_backend_testing::ElementHandle;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, ModelRc, VecModel};

const CAP_HEIGHT: f32 = 20.0;

fn row(position: f32) -> MixerStripRow {
    MixerStripRow {
        id: "in:0@d".into(),
        name: "GUITARRA 3 (DI4000 1 / DI4000 2)".into(),
        detail: "IN 23".into(),
        is_input: true,
        position,
        gain_label: "0.0 dB".into(),
        muted: false,
        soloed: false,
        solo_silenced: false,
    }
}

fn set_strip<C: ComponentHandle>(w: &C, position: f32)
where
    for<'a> MixerBridge<'a>: Global<'a, C>,
{
    let bridge = MixerBridge::get(w);
    bridge.set_inputs(ModelRc::new(VecModel::from(vec![row(position)])));
    bridge.set_outputs(ModelRc::new(VecModel::from(Vec::<MixerStripRow>::new())));
}

fn mixer_panel() -> MixerHarness {
    i_slint_backend_testing::init_no_event_loop();
    let w = MixerHarness::new().unwrap();
    w.window().set_size(slint::LogicalSize::new(600.0, 700.0));
    set_strip(&w, 0.5);
    w.show().unwrap();
    w
}

fn compact_view() -> CompactChainViewWindow {
    i_slint_backend_testing::init_no_event_loop();
    let w = CompactChainViewWindow::new().unwrap();
    w.window().set_size(slint::LogicalSize::new(900.0, 900.0));
    set_strip(&w, 0.5);
    w.show().unwrap();
    // The mixer section opens collapsed; expand it the way the user does.
    let toggle = only(&w, "SectionToggle::area");
    let at = LogicalPosition::new(
        toggle.absolute_position().x + toggle.size().width / 2.0,
        toggle.absolute_position().y + toggle.size().height / 2.0,
    );
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
    w
}

fn only<C: ComponentHandle>(w: &C, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(w, id)
        .next()
        .unwrap_or_else(|| panic!("{id} not found"))
}

fn cap_center_y<C: ComponentHandle>(w: &C) -> f32 {
    let cap = only(w, "MixerStripView::cap");
    cap.absolute_position().y + cap.size().height / 2.0
}

/// Fader position the strip reports for a pointer at absolute `y`.
fn position_at<C: ComponentHandle>(w: &C, y: f32) -> f32 {
    let ta = only(w, "MixerStripView::fader-ta");
    let travel = ta.size().height - CAP_HEIGHT;
    (1.0 - ((y - ta.absolute_position().y) - CAP_HEIGHT / 2.0) / travel).clamp(0.0, 1.0)
}

/// Drag the fader from the cap centre by `step` px `steps` times. After each
/// move, `after_move` runs (the app's redraw, or nothing). Returns the cap
/// centre seen after every move and the positions the strip reported.
fn drag<C: ComponentHandle>(
    w: &C,
    step: f32,
    steps: usize,
    mut after_move: impl FnMut(&C, &[f32]),
) -> (Vec<f32>, Vec<f32>, f32)
where
    for<'a> MixerBridge<'a>: Global<'a, C>,
{
    let reported = Rc::new(RefCell::new(Vec::<f32>::new()));
    let r = reported.clone();
    MixerBridge::get(w).on_fader_moved(move |_, p| r.borrow_mut().push(p));

    let cap = only(w, "MixerStripView::cap");
    let x = cap.absolute_position().x + cap.size().width / 2.0;
    let mut y = cap_center_y(w);
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(x, y),
    });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: LogicalPosition::new(x, y),
        button: PointerEventButton::Left,
    });
    let mut caps = Vec::new();
    for _ in 0..steps {
        y += step;
        win.dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(x, y),
        });
        after_move(w, &reported.borrow());
        caps.push(cap_center_y(w));
    }
    win.dispatch_event(WindowEvent::PointerReleased {
        position: LogicalPosition::new(x, y),
        button: PointerEventButton::Left,
    });
    let got = reported.borrow().clone();
    (caps, got, y)
}

fn assert_cap_follows(start: f32, step: f32, caps: &[f32]) {
    for (i, c) in caps.iter().enumerate() {
        let want = start + step * (i as f32 + 1.0);
        assert!(
            (c - want).abs() < 1.5,
            "cap did not follow the pointer on move {}: cap centre {c}, pointer {want}",
            i + 1
        );
    }
}

fn assert_drop<C: ComponentHandle>(w: &C, reported: &[f32], steps: usize, drop_y: f32) {
    assert!(
        reported.len() >= steps,
        "the drag stopped reporting: {} positions for {steps} moves ({reported:?})",
        reported.len()
    );
    let want = position_at(w, drop_y);
    let last = *reported.last().unwrap();
    assert!(
        (last - want).abs() < 0.01,
        "final position {last} != drop position {want}"
    );
}

/// The app answers every reported position by redrawing the strips from the
/// dispatcher through its real redraw path, as the wiring and the 100 ms poll do.
fn redraw_from_reports<C: ComponentHandle>(w: &C, reported: &[f32])
where
    for<'a> MixerBridge<'a>: Global<'a, C>,
{
    if let Some(p) = reported.last() {
        set_mixer_rows(&MixerBridge::get(w), vec![row(*p)], Vec::new());
    }
}

#[test]
fn the_mixer_window_cap_follows_the_pointer_while_dragging() {
    let w = mixer_panel();
    let start = cap_center_y(&w);
    let (caps, reported, drop_y) = drag(&w, 8.0, 6, |_, _| {});
    assert_cap_follows(start, 8.0, &caps);
    assert_drop(&w, &reported, 6, drop_y);
}

#[test]
fn the_compact_view_cap_follows_the_pointer_while_dragging() {
    let w = compact_view();
    let start = cap_center_y(&w);
    let (caps, reported, drop_y) = drag(&w, -8.0, 6, |_, _| {});
    assert_cap_follows(start, -8.0, &caps);
    assert_drop(&w, &reported, 6, drop_y);
}

#[test]
fn a_mixer_window_drag_survives_the_strips_being_redrawn() {
    let w = mixer_panel();
    let start = cap_center_y(&w);
    let (caps, reported, drop_y) = drag(&w, 8.0, 6, redraw_from_reports);
    assert_cap_follows(start, 8.0, &caps);
    assert_drop(&w, &reported, 6, drop_y);
}

#[test]
fn a_compact_view_drag_survives_the_strips_being_redrawn() {
    let w = compact_view();
    let start = cap_center_y(&w);
    let (caps, reported, drop_y) = drag(&w, -8.0, 6, redraw_from_reports);
    assert_cap_follows(start, -8.0, &caps);
    assert_drop(&w, &reported, 6, drop_y);
}
