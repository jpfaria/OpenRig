//! #915 — the block editor of a native amp. Its ten parameters are one front
//! panel: no tab bar, every knob drawn at once. Two things used to break that.
//! The curated `knob_layout` overlays are drawn in layout order and cover only
//! seven of the ten parameters, so `input` / `bright` / `output` had no control
//! at all; and the schema's seven groups were rendered as seven tabs, five of
//! them holding a single knob, which turned an amp's panel into a filing
//! cabinet.

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

fn native_ctx(effect_type: &str, model_id: &str) -> BlockEditorWindowSetupCtx {
    let seeded = application::block_factory::default_params_for_model(effect_type, model_id)
        .unwrap_or_default();
    BlockEditorWindowSetupCtx {
        chain_index: 0,
        block_index: None,
        before_index: 0,
        instrument: "electric_guitar".to_string(),
        effect_type: effect_type.to_string(),
        model_id: model_id.to_string(),
        enabled: true,
        editor_data: BlockEditorData {
            effect_type: effect_type.to_string(),
            model_id: model_id.to_string(),
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
/// `BlockParamGrid` renders on.
fn drawn_rows(win: &crate::BlockEditorWindow) -> Vec<String> {
    crate::BlockEditorBridge::get(win)
        .get_block_parameter_items()
        .iter()
        .filter(|it| it.tab_slot >= 0)
        .map(|it| it.path.to_string())
        .collect()
}

#[test]
fn a_native_amp_editor_is_one_panel_with_every_knob() {
    i_slint_backend_testing::init_no_event_loop();
    let weak = {
        let w = crate::AppWindow::new().unwrap();
        w.as_weak()
    };
    let (win, _timer) = create_and_wire(weak, native_ctx("amp", "blackface_clean")).unwrap();

    assert_eq!(
        win.get_block_parameter_groups().row_count(),
        0,
        "ten knobs fit the panel — no tab bar"
    );
    // The curated overlays ignore everything but their own order, and cover
    // only seven of the ten parameters, so a block whose layout is incomplete
    // must not publish them.
    assert_eq!(
        crate::BlockEditorBridge::get(&win)
            .get_block_knob_overlays()
            .row_count(),
        0,
    );
    assert_eq!(
        drawn_rows(&win),
        vec![
            "input", "gain", "bass", "middle", "treble", "master", "bright", "sag", "room_mix",
            "output",
        ],
        "every parameter is on the panel — `input`, `bright` and `output` had no control before"
    );
}

#[test]
fn a_native_preamp_editor_is_one_panel_with_every_knob() {
    i_slint_backend_testing::init_no_event_loop();
    let weak = {
        let w = crate::AppWindow::new().unwrap();
        w.as_weak()
    };
    let (win, _timer) = create_and_wire(weak, native_ctx("preamp", "american_clean")).unwrap();

    assert_eq!(win.get_block_parameter_groups().row_count(), 0);
    assert_eq!(
        drawn_rows(&win).len(),
        11,
        "the preamp draws all eleven of its parameters at once"
    );
}

/// The NAM editor window keeps its module tabs — the regression the amp fix
/// nearly took with it.
#[test]
fn a_nam_editor_window_keeps_its_module_tabs() {
    i_slint_backend_testing::init_no_event_loop();
    let weak = {
        let w = crate::AppWindow::new().unwrap();
        w.as_weak()
    };
    let (win, _timer) = create_and_wire(weak, native_ctx("nam", "neural_amp_modeler")).unwrap();

    let groups: Vec<String> = win
        .get_block_parameter_groups()
        .iter()
        .map(|g| g.to_string())
        .collect();
    assert_eq!(groups, vec!["Main", "Amp", "Noise Gate", "EQ"]);
    assert_eq!(
        drawn_rows(&win).len(),
        2,
        "the window opens on the first tab and draws its two rows"
    );
}
