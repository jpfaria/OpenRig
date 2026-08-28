//! #915 — the block editor of a native amp: the tab bar must actually filter
//! the grid. The curated `knob_layout` overlays are drawn by loop index and
//! ignore the active tab, so while they were published every tab drew the same
//! knobs (POWER listed GAIN and the EQ knobs) and `input` / `bright` / `output`
//! — absent from the curated layout — had no control at all.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Model, VecModel};

use crate::block_editor_window_setup::{create_and_wire, BlockEditorWindowSetupCtx};
use crate::project_ops::create_new_project_session;
use crate::state::{BlockEditorData, ProjectSession};

fn empty_session() -> Rc<RefCell<Option<ProjectSession>>> {
    let tmp = tempfile::TempDir::new().unwrap();
    let session = create_new_project_session(&tmp.path().join("config.yaml"));
    std::mem::forget(tmp);
    Rc::new(RefCell::new(Some(session)))
}

fn native_amp_ctx() -> BlockEditorWindowSetupCtx {
    let seeded = application::block_factory::default_params_for_model("amp", "blackface_clean")
        .unwrap_or_default();
    BlockEditorWindowSetupCtx {
        chain_index: 0,
        block_index: None,
        before_index: 0,
        instrument: "electric_guitar".to_string(),
        effect_type: "amp".to_string(),
        model_id: "blackface_clean".to_string(),
        enabled: true,
        editor_data: BlockEditorData {
            effect_type: "amp".to_string(),
            model_id: "blackface_clean".to_string(),
            params: seeded,
            enabled: true,
            is_select: false,
            select_options: Vec::new(),
            selected_select_option_block_id: None,
        },
        block_id: None,
        project_session: empty_session(),
        project_chains: Rc::new(VecModel::default()),
        block_stream_reads: Rc::new(application::live_source::NoLiveSource),
        saved_project_snapshot: Rc::new(RefCell::new(None)),
        project_dirty: Rc::new(RefCell::new(false)),
        input_chain_devices: Rc::new(RefCell::new(Vec::new())),
        output_chain_devices: Rc::new(RefCell::new(Vec::new())),
        selected_block: Rc::new(RefCell::new(None)),
        open_block_windows: Rc::new(RefCell::new(Vec::new())),
        plugin_info_window: Rc::new(RefCell::new(None)),
        open_compact_window: Rc::new(RefCell::new(None)),
        auto_save: false,
    }
}

/// The rows the grid actually draws: `tab_slot >= 0` is exactly the condition
/// `BlockParamGrid` renders on when the block has tabs.
fn drawn_rows(win: &crate::BlockEditorWindow) -> Vec<String> {
    crate::BlockEditorBridge::get(win)
        .get_block_parameter_items()
        .iter()
        .filter(|it| it.tab_slot >= 0)
        .map(|it| it.path.to_string())
        .collect()
}

fn groups(win: &crate::BlockEditorWindow) -> Vec<String> {
    win.get_block_parameter_groups()
        .iter()
        .map(|g| g.to_string())
        .collect()
}

#[test]
fn clicking_a_native_amp_tab_shows_only_that_tabs_knobs() {
    i_slint_backend_testing::init_no_event_loop();
    let weak = {
        let w = crate::AppWindow::new().unwrap();
        w.as_weak()
    };
    let (win, _timer) = create_and_wire(weak, native_amp_ctx()).unwrap();

    // The curated overlays ignore the tab bar, so a tabbed block must publish
    // none — otherwise the grid draws them and the tabs filter nothing.
    assert_eq!(
        crate::BlockEditorBridge::get(&win)
            .get_block_knob_overlays()
            .row_count(),
        0,
        "a native amp has tabs, so it must not publish curated knob overlays"
    );

    let groups = groups(&win);
    assert_eq!(
        groups,
        vec!["Input", "Amp", "EQ", "Power", "Switches", "Cab", "Output"],
    );
    assert_eq!(drawn_rows(&win), vec!["input"], "the editor opens on tab 1");

    // Click POWER — the same call the tab bar's `select(i)` makes.
    let power = groups.iter().position(|g| g == "Power").unwrap() as i32;
    win.invoke_select_parameter_group(power);
    assert_eq!(win.get_active_parameter_group(), power);
    assert_eq!(
        drawn_rows(&win),
        vec!["master", "sag"],
        "POWER must show the power knobs only — not GAIN and the EQ knobs (#915)"
    );

    // Every other tab draws its own group, and `bright` / `output` — which the
    // curated layout omitted entirely — are now reachable.
    for (group, expected) in [
        ("EQ", vec!["bass", "middle", "treble"]),
        ("Amp", vec!["gain"]),
        ("Cab", vec!["room_mix"]),
        ("Switches", vec!["bright"]),
        ("Output", vec!["output"]),
    ] {
        let i = groups.iter().position(|g| g == group).unwrap() as i32;
        win.invoke_select_parameter_group(i);
        assert_eq!(drawn_rows(&win), expected, "tab {group}");
    }
}
