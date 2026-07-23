//! #780 — switching the VST3 plugin in the block editor must REBUILD the tab
//! bar (and reset to the first tab), not leave the previous plugin's tabs
//! stale. This is the "troco de plugin e as abas não são refeitas" bug:
//! `apply_param_tabs` is idempotent, so calling it again for a different plugin
//! fully replaces the groups + active tab + visible params.

use adapter_gui::block_editor_param_tabs::{apply_param_tabs, TabState};
use adapter_gui::{BlockEditorWindow, BlockParameterItem};
use slint::{Model, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

fn item(label: &str, group: &str) -> BlockParameterItem {
    BlockParameterItem {
        label: label.into(),
        group: group.into(),
        ..Default::default()
    }
}

#[test]
fn switching_models_rebuilds_the_tabs_and_resets_active() {
    i_slint_backend_testing::init_no_event_loop();

    let win = BlockEditorWindow::new().unwrap();
    let items = Rc::new(VecModel::<BlockParameterItem>::default());
    win.set_block_parameter_items(slint::ModelRc::from(items.clone()));
    let state = Rc::new(RefCell::new(TabState::default()));

    // Plugin A: two groups (Tone, Voicing) → a two-tab bar.
    apply_param_tabs(
        &win,
        &items,
        &state,
        vec![
            item("Gain", "Tone"),
            item("Level", "Tone"),
            item("Mode", "Voicing"),
        ],
    );
    assert_eq!(
        win.get_block_parameter_groups().row_count(),
        2,
        "plugin A must expose two tabs"
    );
    // win_param_items must stay FULL (all 3 params, both tabs) so a save
    // (OverwriteBlock) never drops the non-active tab's params. Only the
    // active tab (Tone: 2) is visible via tab_slot >= 0.
    assert_eq!(
        items.row_count(),
        3,
        "win_param_items must keep every tab's params for persistence"
    );
    let visible = (0..items.row_count())
        .filter(|&i| {
            items
                .row_data(i)
                .map(|it| it.tab_slot >= 0)
                .unwrap_or(false)
        })
        .count();
    assert_eq!(
        visible, 2,
        "only the active tab's params are shown (tab_slot >= 0)"
    );
    // Move to the second tab, as a user would.
    win.set_active_parameter_group(1);

    // Switch to plugin B: a single ungrouped set → one group ("Main"), no bar.
    apply_param_tabs(
        &win,
        &items,
        &state,
        vec![item("Mix", ""), item("Feedback", "")],
    );
    assert_eq!(
        win.get_block_parameter_groups().row_count(),
        1,
        "switching plugins must REBUILD the tabs for the new plugin, not keep A's two tabs"
    );
    assert_eq!(
        win.get_active_parameter_group(),
        0,
        "switching plugins must reset to the first tab"
    );
    // win_param_items now holds plugin B's full params (both would be saved).
    assert_eq!(
        items.row_count(),
        2,
        "win_param_items now holds plugin B's params (full), not A's"
    );
}
