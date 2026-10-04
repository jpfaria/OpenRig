//! #328 (spec §5.3) — the checklist overlay under real pointer events, and
//! the wiring end to end (open → rows → toggle → command, rows stay listed).

use std::cell::RefCell;
use std::rc::Rc;

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, Model, ModelRc, Timer, VecModel};

use application::command::{ChainCommand, Command};

use crate::chain_graph_fixtures_tests::{chain, core, devices, recording_session, rows};
use crate::chain_graph_ids::{INPUT_NODE_ID, OUTPUT_NODE_ID};
use crate::endpoint_checklist_wiring::{wire, EndpointChecklistWiringCtx};
use crate::{ChainGraphOverlayState, ChannelOptionItem, EndpointChecklistHarness};

fn click_at(w: &impl ComponentHandle, p: LogicalPosition) {
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: p });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: p,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerReleased {
        position: p,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerExited);
}

fn centre(el: &i_slint_backend_testing::ElementHandle) -> LogicalPosition {
    let (pos, size) = (el.absolute_position(), el.size());
    LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0)
}

fn item(index: i32, label: &str, selected: bool) -> ChannelOptionItem {
    ChannelOptionItem {
        index,
        label: label.into(),
        selected,
        available: true,
    }
}

fn open(h: &EndpointChecklistHarness) {
    let state = ChainGraphOverlayState::get(h);
    state.set_checklist_items(ModelRc::new(VecModel::from(vec![
        item(0, "In 1", true),
        item(1, "In 2", true),
    ])));
    state.set_checklist_chain_index(0);
    state.set_checklist_node(INPUT_NODE_ID.into());
    state.set_checklist_title("Inputs".into());
    state.set_checklist_open(true);
}

#[test]
fn clicking_a_row_reports_the_node_the_row_and_the_new_state() {
    i_slint_backend_testing::init_no_event_loop();
    let h = EndpointChecklistHarness::new().unwrap();
    open(&h);
    let got: Rc<RefCell<Option<(i32, String, i32, bool)>>> = Rc::new(RefCell::new(None));
    let seen = got.clone();
    ChainGraphOverlayState::get(&h).on_checklist_toggled(move |ci, node, row, on| {
        *seen.borrow_mut() = Some((ci, node.to_string(), row, on));
    });
    h.show().unwrap();
    let cell =
        i_slint_backend_testing::ElementHandle::find_by_element_id(&h, "ChannelPicker::chan-cell")
            .nth(1)
            .expect("the second endpoint row");
    click_at(&h, centre(&cell));
    assert_eq!(
        *got.borrow(),
        Some((0, INPUT_NODE_ID.to_string(), 1, false))
    );
}

#[test]
fn a_click_outside_the_card_closes_the_checklist() {
    i_slint_backend_testing::init_no_event_loop();
    let h = EndpointChecklistHarness::new().unwrap();
    open(&h);
    h.show().unwrap();
    click_at(&h, LogicalPosition::new(5.0, 5.0));
    assert!(!ChainGraphOverlayState::get(&h).get_checklist_open());
}

#[test]
fn opening_the_input_node_lists_the_chains_inputs_and_a_toggle_dispatches() {
    i_slint_backend_testing::init_no_event_loop();
    let app = crate::AppWindow::new().unwrap();
    let (session, recorder) = recording_session(vec![chain(vec![core("amp")])]);
    wire(
        &app,
        EndpointChecklistWiringCtx {
            project_session: session,
            project_chains: rows(),
            input_chain_devices: Rc::new(RefCell::new(devices())),
            output_chain_devices: Rc::new(RefCell::new(devices())),
            toast_timer: Rc::new(Timer::default()),
        },
    );
    let state = ChainGraphOverlayState::get(&app);
    state.invoke_open_checklist(0, INPUT_NODE_ID.into());
    assert!(state.get_checklist_open());
    assert_eq!(
        state.get_checklist_title().to_string(),
        rust_i18n::t!("title-endpoints-input").to_string()
    );
    let labels: Vec<String> = state
        .get_checklist_items()
        .iter()
        .map(|i| i.label.to_string())
        .collect();
    assert_eq!(
        labels,
        vec!["Quantum HD 8 · In 1/2", "Quantum HD 8 · In 3/4"]
    );

    state.invoke_checklist_toggled(0, INPUT_NODE_ID.into(), 1, false);

    assert!(matches!(
        &recorder.seen.borrow()[0],
        Command::Chain(ChainCommand::SetChainEndpointEnabled { enabled: false, .. })
    ));
    assert_eq!(
        state.get_checklist_items().row_count(),
        2,
        "an unchecked endpoint stays listed"
    );
}

#[test]
fn the_card_shows_only_the_title_and_the_rows() {
    i_slint_backend_testing::init_no_event_loop();
    let h = EndpointChecklistHarness::new().unwrap();
    open(&h);
    h.show().unwrap();
    let labels: Vec<String> = i_slint_backend_testing::ElementQuery::from_root(&h)
        .match_descendants()
        .find_all()
        .into_iter()
        .filter_map(|el| el.accessible_label())
        .map(|s| s.to_string())
        .collect();
    assert!(
        !labels.iter().any(|l| l.contains("hint-endpoint-checklist")
            || l.starts_with("Unchecked endpoints")
            || l.starts_with("Endpoints desmarcados")
            || l.ends_with(" endpoints")
            || l.ends_with(" endpoint")),
        "no explanatory text and no endpoint count: {labels:?}"
    );
}

#[test]
fn the_card_closes_with_the_x_button_like_every_panel() {
    i_slint_backend_testing::init_no_event_loop();
    let h = EndpointChecklistHarness::new().unwrap();
    open(&h);
    h.show().unwrap();
    let close = i_slint_backend_testing::ElementHandle::find_by_element_id(
        &h,
        "EndpointChecklistOverlay::close-x",
    )
    .next()
    .expect("the X close button");
    assert_eq!(
        close.accessible_role(),
        Some(i_slint_backend_testing::AccessibleRole::Button)
    );
    let worded_close = i_slint_backend_testing::ElementQuery::from_root(&h)
        .match_descendants()
        .match_accessible_role(i_slint_backend_testing::AccessibleRole::Text)
        .find_all()
        .into_iter()
        .filter_map(|el| el.accessible_label())
        .any(|l| l == "Fechar" || l == "Close" || l == "btn-close");
    assert!(!worded_close, "no worded Close button");
    click_at(&h, centre(&close));
    assert!(!ChainGraphOverlayState::get(&h).get_checklist_open());
}

#[test]
fn opening_the_output_node_names_its_rows_from_the_output_devices() {
    i_slint_backend_testing::init_no_event_loop();
    let app = crate::AppWindow::new().unwrap();
    let (session, _recorder) = recording_session(vec![chain(vec![core("amp")])]);
    wire(
        &app,
        EndpointChecklistWiringCtx {
            project_session: session,
            project_chains: rows(),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(devices())),
            toast_timer: Rc::new(Timer::default()),
        },
    );
    let state = ChainGraphOverlayState::get(&app);
    state.invoke_open_checklist(0, OUTPUT_NODE_ID.into());
    assert!(state.get_checklist_open());
    let labels: Vec<String> = state
        .get_checklist_items()
        .iter()
        .map(|i| i.label.to_string())
        .collect();
    assert_eq!(
        labels,
        vec!["Quantum HD 8 · Out 1/2", "Quantum HD 8 · Out 3/4"]
    );
}
