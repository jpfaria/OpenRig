//! #398 — nothing leaves the window. A panel, list or hover label that does
//! not fit where it opens (below its button) opens the other way; one taller
//! than the window is held inside it and scrolls. Nothing is ever cut off by
//! the window edge.

use adapter_gui::{LooperItem, LooperOverlayHarness, OverlayPlacementHarness};
use i_slint_backend_testing::ElementHandle;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::path::{Path, PathBuf};

struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn rect(w: &impl ComponentHandle, id: &str) -> Rect {
    let el = ElementHandle::find_by_element_id(w, id)
        .next()
        .unwrap_or_else(|| panic!("{id} is on screen"));
    let p = el.absolute_position();
    let s = el.size();
    Rect {
        x: p.x,
        y: p.y,
        w: s.width,
        h: s.height,
    }
}

fn assert_inside(r: &Rect, win_w: f32, win_h: f32, what: &str) {
    assert!(
        r.x >= -0.5 && r.y >= -0.5 && r.x + r.w <= win_w + 0.5 && r.y + r.h <= win_h + 0.5,
        "{what} leaves the {win_w}x{win_h} window: x {} y {} w {} h {}",
        r.x,
        r.y,
        r.w,
        r.h
    );
}

fn looper(uid: i32) -> LooperItem {
    LooperItem {
        uid,
        state_code: 2,
        progress: 0.25,
        time_label: "0:02 / 0:08".into(),
        layers: 2,
        mix: 100,
        decay: 100,
        speed_index: 1,
        reverse: false,
        can_undo: true,
        can_redo: true,
        can_record: true,
        can_edit: false,
        input_index: 0,
        output_index: 0,
        preset_index: 0,
    }
}

/// The looper panel opened from a loop button whose top edge is at
/// `button_top` and whose left edge is at `button_x`, in a 900 × `win_h`
/// window.
fn looper_panel(count: i32, win_h: f32, button_x: f32, button_top: f32) -> LooperOverlayHarness {
    let w = LooperOverlayHarness::new().unwrap();
    w.set_win_h(win_h);
    w.set_expanded_uid(-1);
    w.set_loopers(ModelRc::new(VecModel::from(
        (0..count).map(looper).collect::<Vec<_>>(),
    )));
    w.set_anchor_x(button_x - 572.0);
    w.set_anchor_top(button_top);
    w.set_anchor_bottom(button_top + 32.0);
    w.invoke_open_at();
    w.show().unwrap();
    w
}

#[test]
fn a_panel_with_room_below_its_button_opens_below() {
    i_slint_backend_testing::init_no_event_loop();
    let w = looper_panel(1, 640.0, 800.0, 60.0);
    let panel = rect(&w, "LooperOverlay::panel");
    assert_inside(&panel, 900.0, 640.0, "the looper panel");
    assert!(
        (panel.y - 92.0).abs() < 0.5,
        "below the button: {}",
        panel.y
    );
}

#[test]
fn a_panel_with_no_room_below_its_button_opens_above_it() {
    i_slint_backend_testing::init_no_event_loop();
    let w = looper_panel(1, 640.0, 800.0, 560.0);
    let panel = rect(&w, "LooperOverlay::panel");
    assert_inside(&panel, 900.0, 640.0, "the looper panel");
    assert!(
        panel.y + panel.h <= 560.5,
        "the panel opens above the button (its bottom at {}, the button's top at 560)",
        panel.y + panel.h
    );
}

#[test]
fn a_panel_near_the_left_edge_stays_inside() {
    i_slint_backend_testing::init_no_event_loop();
    let w = looper_panel(1, 640.0, 40.0, 60.0);
    let panel = rect(&w, "LooperOverlay::panel");
    assert_inside(&panel, 900.0, 640.0, "the looper panel");
}

#[test]
fn a_panel_taller_than_the_window_is_held_inside_and_scrolls() {
    i_slint_backend_testing::init_no_event_loop();
    let w = looper_panel(10, 360.0, 800.0, 100.0);
    let panel = rect(&w, "LooperOverlay::panel");
    assert_inside(&panel, 900.0, 360.0, "the looper panel");
    assert_eq!(
        ElementHandle::find_by_element_id(&w, "LooperPanelView::scroll").count(),
        1,
        "the loopers scroll under the panel's bar instead of being cut off"
    );
}

fn tip_harness(host_x: f32, host_y: f32) -> OverlayPlacementHarness {
    let w = OverlayPlacementHarness::new().unwrap();
    w.set_host_x(host_x);
    w.set_host_y(host_y);
    w.show().unwrap();
    slint::platform::update_timers_and_animations();
    w
}

#[test]
fn a_hover_label_at_the_bottom_right_corner_opens_above_and_inside() {
    i_slint_backend_testing::init_no_event_loop();
    let w = tip_harness(372.0, 272.0);
    let tip = rect(&w, "HoverTipLayer::tip");
    assert_inside(&tip, 400.0, 300.0, "the hover label");
    assert!(
        tip.y + tip.h <= 272.5,
        "above its button: bottom {}",
        tip.y + tip.h
    );
}

#[test]
fn a_hover_label_at_the_top_left_corner_opens_below_and_inside() {
    i_slint_backend_testing::init_no_event_loop();
    let w = tip_harness(0.0, 0.0);
    let tip = rect(&w, "HoverTipLayer::tip");
    assert_inside(&tip, 400.0, 300.0, "the hover label");
    assert!(tip.y >= 27.5, "below its button: top {}", tip.y);
}

#[test]
fn a_dialog_taller_than_the_window_is_held_inside_and_scrolls() {
    i_slint_backend_testing::init_no_event_loop();
    let w = OverlayPlacementHarness::new().unwrap();
    w.set_dialog_open(true);
    w.show().unwrap();
    let card = rect(&w, "DialogCard::card");
    assert_inside(&card, 400.0, 300.0, "the dialog");
    assert_eq!(
        ElementHandle::find_by_element_id(&w, "DialogCard::scroll").count(),
        1,
        "a dialog taller than the window scrolls instead of being cut off"
    );
}

// ── Every overlay of the app follows the same rule ─────────────────────

fn ui_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui")
}

fn ui(rel: &str) -> String {
    let path = ui_dir().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn app_slint_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.is_dir() {
            if name != "modules" {
                app_slint_files(&path, out);
            }
        } else if name.ends_with(".slint") && !name.contains("harness") && !name.contains("mockup")
        {
            out.push(path);
        }
    }
}

#[test]
fn every_panel_opened_from_a_button_is_placed_inside_the_window() {
    for file in [
        "components/looper_overlay.slint",
        "components/di_loop_overlay.slint",
        "components/tone_doctor_overlay.slint",
    ] {
        let src = ui(file);
        assert!(src.contains("OverlayPlacement.top("), "{file} flips");
        assert!(
            src.contains("OverlayPlacement.left("),
            "{file} stays inside sideways"
        );
        assert!(
            src.contains("OverlayPlacement.fit("),
            "{file} is capped to the window"
        );
    }
    assert!(ui("pages/compact_chain_view.slint").contains("OverlayPlacement.top("));
}

#[test]
fn every_list_that_drops_from_a_field_opens_upward_when_it_does_not_fit() {
    for file in [
        "components/select.slint",
        "components/preset_select.slint",
        "components/model_select_with_search.slint",
        "components/chain_volume_button.slint",
        "components/language_selector.slint",
    ] {
        assert!(ui(file).contains("OverlayPlacement.top("), "{file}");
    }
}

#[test]
fn every_hover_label_is_the_contained_tooltip() {
    for file in [
        "components/chain_chips.slint",
        "pages/project_launcher.slint",
    ] {
        let src = ui(file);
        assert!(
            !src.contains("root.tooltip != \"\" : Rectangle"),
            "{file} draws its own hover label"
        );
        assert!(src.contains("IconTooltip {"), "{file}");
    }
    assert!(ui("components/strip_label.slint").contains("OverlayPlacement.top"));
    assert!(ui("components/language_selector.slint").contains("IconTooltip {"));
}

#[test]
fn every_window_publishes_its_bounds() {
    let mut files = Vec::new();
    app_slint_files(&ui_dir(), &mut files);
    for path in files {
        let src = std::fs::read_to_string(&path).unwrap();
        if !src.contains("inherits Window {") {
            continue;
        }
        let rel = path.strip_prefix(ui_dir()).unwrap().display().to_string();
        let windows = src.matches("inherits Window {").count();
        assert_eq!(
            src.matches("WindowBoundsProbe {").count(),
            windows,
            "{rel}: each window tells its overlays where its edges are"
        );
        assert_eq!(
            src.matches("HoverTipLayer {").count(),
            windows,
            "{rel}: each window draws its hover labels above everything"
        );
    }
}
