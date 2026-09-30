//! Responsibility: proves every chain meter row tells the input meter apart from the output meter
//!
//! #1006 — since #928 each meter row names the E/S binding it reads from, and a
//! binding carries ONE name for both directions. The row then showed
//! "GUITARRA 1 - SYN5050" on the input meter AND on the output meter, so the
//! user could not tell which bar was the input and which was the output. Each
//! meter must carry its own direction tag, whatever the binding is called.
//!
//! Both views that draw the rows are covered: the chain card (`ChainRowMeters`)
//! and the compact view footer (`CompactStreamMeters`). One `#[test]` because
//! the slint testing backend is per-thread.

use crate::{AppWindow, CompactChainViewWindow, ProjectChainItem, StreamMeter};
use slint::{ComponentHandle, LogicalSize, ModelRc, VecModel};

const BINDING: &str = "GUITARRA 1 - SYN5050";

fn same_name_meter() -> StreamMeter {
    StreamMeter {
        in_dbfs: -20.0,
        out_dbfs: -12.0,
        in_label: BINDING.into(),
        out_label: BINDING.into(),
    }
}

fn labels(w: &impl ComponentHandle, id: &str) -> Vec<String> {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id)
        .filter_map(|el| el.accessible_label())
        .map(|s| s.to_string())
        .collect()
}

/// One direction tag per meter, and the two tags say different things — the
/// binding name alone (shared by both meters) is not a direction.
fn assert_directions(w: &impl ComponentHandle, component: &str) {
    let input = labels(w, &format!("{component}::in-direction"));
    let output = labels(w, &format!("{component}::out-direction"));
    assert_eq!(
        input.len(),
        1,
        "{component}: the input meter carries no direction tag"
    );
    assert_eq!(
        output.len(),
        1,
        "{component}: the output meter carries no direction tag"
    );
    assert_ne!(
        input[0], output[0],
        "{component}: input and output tags read the same"
    );
    assert!(
        !input[0].is_empty() && input[0] != BINDING && output[0] != BINDING,
        "{component}: a direction tag repeats the binding name"
    );
}

#[test]
fn meter_rows_tag_input_and_output_even_when_the_binding_name_is_shared() {
    i_slint_backend_testing::init_no_event_loop();

    let app = AppWindow::new().unwrap();
    app.window().set_size(LogicalSize::new(1400.0, 900.0));
    let chain = ProjectChainItem {
        title: "Chain".into(),
        enabled: true,
        stream_meters: ModelRc::new(VecModel::from(vec![same_name_meter()])),
        ..Default::default()
    };
    app.set_project_chains(ModelRc::new(VecModel::from(vec![chain])));
    app.set_show_project_chains(true);
    app.show().unwrap();
    assert_directions(&app, "ChainRowMeters");

    let compact = CompactChainViewWindow::new().unwrap();
    compact.window().set_size(LogicalSize::new(1400.0, 900.0));
    compact.set_stream_meters(ModelRc::new(VecModel::from(vec![same_name_meter()])));
    compact.show().unwrap();
    assert_directions(&compact, "CompactStreamMeters");
}
