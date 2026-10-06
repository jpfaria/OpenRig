//! #398 — a tab row wider than its bar scrolls sideways; while more tabs wait
//! past the right edge, that edge fades out so the cut tab reads as "more
//! this way", never as a broken label.

use adapter_gui::ParamTabBarHarness;
use i_slint_backend_testing::ElementHandle;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

fn bar(groups: &[&str]) -> ParamTabBarHarness {
    let w = ParamTabBarHarness::new().unwrap();
    w.set_groups(ModelRc::new(VecModel::from(
        groups
            .iter()
            .map(|g| SharedString::from(*g))
            .collect::<Vec<_>>(),
    )));
    w.show().unwrap();
    w
}

#[test]
fn a_row_that_overflows_its_bar_fades_at_the_right_edge() {
    i_slint_backend_testing::init_no_event_loop();
    let names: Vec<String> = (1..=14).map(|i| format!("Group {i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let w = bar(&names);
    assert_eq!(
        ElementHandle::find_by_element_id(&w, "ParamTabBar::more").count(),
        1
    );
}

#[test]
fn a_row_that_fits_has_no_fade() {
    i_slint_backend_testing::init_no_event_loop();
    let w = bar(&["Capture", "Amp", "Noise Gate"]);
    assert_eq!(
        ElementHandle::find_by_element_id(&w, "ParamTabBar::more").count(),
        0
    );
}
