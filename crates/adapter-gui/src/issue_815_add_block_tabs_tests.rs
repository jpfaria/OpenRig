//! #815 — a block ADDED to the chain must open the same tabbed editor as an
//! edited block. The ADD flow now goes through `create_and_wire` in
//! "new-block" mode (`block_index: None`): edit-mode off, "add" confirm label,
//! and the #780 parameter tabs populated exactly like the edit path.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Model, VecModel};

use crate::block_editor_window_setup::{create_and_wire, BlockEditorWindowSetupCtx};
use crate::project_ops::create_new_project_session;
use crate::state::{BlockEditorData, ProjectSession};

fn empty_session() -> Rc<RefCell<Option<ProjectSession>>> {
    let tmp = tempfile::TempDir::new().unwrap();
    let session = create_new_project_session(&tmp.path().join("config.yaml"));
    // The window setup only wires callbacks against the session; it never reads
    // it during construction. Leak the tempdir so the session's paths stay valid
    // for the lifetime of the test.
    std::mem::forget(tmp);
    Rc::new(RefCell::new(Some(session)))
}

fn new_block_ctx() -> BlockEditorWindowSetupCtx {
    // The 8-band parametric EQ exposes 8 parameter groups ("Band 1".."Band 8"),
    // so a correctly built editor must render more than one tab.
    let seeded =
        application::block_factory::default_params_for_model("filter", "eq_eight_band_parametric")
            .unwrap_or_default();
    BlockEditorWindowSetupCtx {
        chain_index: 0,
        block_index: None,
        before_index: 0,
        instrument: "electric_guitar".to_string(),
        effect_type: "filter".to_string(),
        model_id: "eq_eight_band_parametric".to_string(),
        enabled: true,
        editor_data: BlockEditorData {
            effect_type: "filter".to_string(),
            model_id: "eq_eight_band_parametric".to_string(),
            params: seeded,
            enabled: true,
            is_select: false,
            select_options: Vec::new(),
            selected_select_option_block_id: None,
        },
        block_id: None,
        project_session: empty_session(),
        project_chains: Rc::new(VecModel::default()),
        project_runtime: Rc::new(RefCell::new(None)),
        saved_project_snapshot: Rc::new(RefCell::new(None)),
        project_dirty: Rc::new(RefCell::new(false)),
        input_chain_devices: Rc::new(RefCell::new(Vec::new())),
        output_chain_devices: Rc::new(RefCell::new(Vec::new())),
        selected_block: Rc::new(RefCell::new(None)),
        open_block_windows: Rc::new(RefCell::new(Vec::new())),
        plugin_info_window: Rc::new(RefCell::new(None)),
        auto_save: false,
    }
}

#[test]
fn adding_a_block_opens_the_tabbed_editor_in_add_mode() {
    i_slint_backend_testing::init_no_event_loop();
    let weak = {
        let w = crate::AppWindow::new().unwrap();
        w.as_weak()
    };
    let (win, _timer) = create_and_wire(weak, new_block_ctx()).unwrap();

    // The #780 parameter tabs must be built for a NEW block, just like edit.
    assert!(
        win.get_block_parameter_groups().row_count() > 1,
        "a newly added 8-band EQ must show its parameter tabs"
    );
    // New block => add mode, not edit mode (no delete, confirm = add).
    assert!(
        !win.get_block_drawer_edit_mode(),
        "adding a block must NOT be in edit mode"
    );
}
