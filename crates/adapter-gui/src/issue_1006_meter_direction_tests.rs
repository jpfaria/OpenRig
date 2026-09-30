//! Responsibility: proves every chain meter row tells the input meter apart from the output meter
//!
//! #1006 — since #928 each meter row names the E/S binding it reads from, and a
//! binding carries ONE name for both directions, so "GUITARRA 1 - SYN5050" sat
//! on the input bar AND on the output bar. Each bar keeps the binding name and
//! adds its own direction and channels: "IN 1" on the input, "OUT 17,18" on
//! the output.
//!
//! Both views that draw the rows are covered: the chain card (`ChainRowMeters`)
//! and the compact view footer (`CompactStreamMeters`). One `#[test]` because
//! the slint testing backend is per-thread.

use crate::{AppWindow, CompactChainViewWindow, ProjectChainItem, StreamMeter};
use slint::{ComponentHandle, LogicalSize, ModelRc, VecModel};

const BINDING: &str = "GUITARRA 1 - SYN5050";

fn meter() -> StreamMeter {
    StreamMeter {
        in_dbfs: -20.0,
        out_dbfs: -12.0,
        in_label: BINDING.into(),
        out_label: BINDING.into(),
        in_channels: "1".into(),
        out_channels: "17,18".into(),
    }
}

fn texts(w: &impl ComponentHandle, id: &str) -> Vec<String> {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id)
        .filter_map(|el| el.accessible_label())
        .map(|s| s.to_string())
        .collect()
}

fn assert_row(w: &impl ComponentHandle, component: &str) {
    for (id, want) in [
        ("in-name", BINDING),
        ("in-channels", "IN 1"),
        ("out-name", BINDING),
        ("out-channels", "OUT 17,18"),
    ] {
        assert_eq!(
            texts(w, &format!("{component}::{id}")),
            vec![want.to_string()],
            "{component}::{id}"
        );
    }
}

#[test]
fn meter_rows_show_the_binding_name_and_the_channels_of_each_side() {
    i_slint_backend_testing::init_no_event_loop();

    let app = AppWindow::new().unwrap();
    app.window().set_size(LogicalSize::new(1400.0, 900.0));
    let chain = ProjectChainItem {
        title: "Chain".into(),
        enabled: true,
        stream_meters: ModelRc::new(VecModel::from(vec![meter()])),
        ..Default::default()
    };
    app.set_project_chains(ModelRc::new(VecModel::from(vec![chain])));
    app.set_show_project_chains(true);
    app.show().unwrap();
    assert_row(&app, "ChainRowMeters");

    let compact = CompactChainViewWindow::new().unwrap();
    compact.window().set_size(LogicalSize::new(1400.0, 900.0));
    compact.set_stream_meters(ModelRc::new(VecModel::from(vec![meter()])));
    compact.show().unwrap();
    assert_row(&compact, "CompactStreamMeters");
}

#[test]
fn the_row_payload_carries_the_channels_of_each_side() {
    let labels = vec![engine::stream_io_labels::StreamIoLabels {
        input: BINDING.into(),
        output: BINDING.into(),
        input_channels: "1".into(),
        output_channels: "17,18".into(),
    }];

    let rows = crate::meter_wiring::rebuild_stream_meters_row(&[], 1, &labels, 100.0, true);

    assert_eq!(rows[0].in_channels.to_string(), "1");
    assert_eq!(rows[0].out_channels.to_string(), "17,18");
}
