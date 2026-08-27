//! #815 — a block ADDED to the chain must open the same tabbed editor as an
//! edited block. The ADD flow now goes through `create_and_wire` in
//! "new-block" mode (`block_index: None`): edit-mode off, "add" confirm label,
//! and the #780 parameter tabs populated exactly like the edit path.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Model, VecModel};

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
    // Tube saturation exposes three parameter groups ("Gain", "Character",
    // "Output"), so a correctly built editor must render more than one tab.
    // It has to be a block the grid draws itself: a block whose EQ widget draws
    // its bands publishes no groups at all (#878).
    let seeded = application::block_factory::default_params_for_model("gain", "tube_saturation")
        .unwrap_or_default();
    BlockEditorWindowSetupCtx {
        chain_index: 0,
        block_index: None,
        before_index: 0,
        instrument: "electric_guitar".to_string(),
        effect_type: "gain".to_string(),
        model_id: "tube_saturation".to_string(),
        enabled: true,
        editor_data: BlockEditorData {
            effect_type: "gain".to_string(),
            model_id: "tube_saturation".to_string(),
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
        "a newly added grouped block must show its parameter tabs"
    );
    // New block => add mode, not edit mode (no delete, confirm = add).
    assert!(
        !crate::BlockEditorBridge::get(&win).get_block_drawer_edit_mode(),
        "adding a block must NOT be in edit mode"
    );
}

/// #85: the owner cannot place an `Output` (nor an `Input`) block — picking the
/// type does nothing. The type IS offered by the picker, so the ADD flow must
/// build an editor for it exactly like any other type; a type that cannot build
/// its editor can never be added.
fn io_block_ctx(effect_type: &str) -> BlockEditorWindowSetupCtx {
    let mut ctx = new_block_ctx();
    ctx.effect_type = effect_type.to_string();
    ctx.model_id = "standard".to_string();
    ctx.editor_data.effect_type = effect_type.to_string();
    ctx.editor_data.model_id = "standard".to_string();
    ctx.editor_data.params = Default::default();
    ctx
}

#[test]
fn adding_an_output_block_opens_its_editor() {
    i_slint_backend_testing::init_no_event_loop();
    let weak = {
        let w = crate::AppWindow::new().unwrap();
        w.as_weak()
    };
    let built = create_and_wire(weak, io_block_ctx("output"));
    assert!(
        built.is_ok(),
        "picking OUTPUT built no editor, so the block is never added — \
         the user clicks and nothing happens (#85)"
    );
}

#[test]
fn adding_an_input_block_opens_its_editor() {
    i_slint_backend_testing::init_no_event_loop();
    let weak = {
        let w = crate::AppWindow::new().unwrap();
        w.as_weak()
    };
    let built = create_and_wire(weak, io_block_ctx("input"));
    assert!(
        built.is_ok(),
        "picking INPUT built no editor, so the block is never added (#85)"
    );
}

/// The end-to-end add flow needs a fully wired `AppWindow`, which the codebase
/// keeps out of tests, so we pin the routing by source (the `no_native_dialogs`
/// convention): the ADD detached branch must build the editor via
/// `create_and_wire`, NOT sync the retired persistent window.
#[test]
fn add_flow_uses_create_and_wire_not_the_persistent_window() {
    let src = include_str!("block_choose_type_callback.rs");
    assert!(
        src.contains("create_and_wire"),
        "the ADD detached branch must build the editor via create_and_wire"
    );
    assert!(
        !src.contains("sync_block_editor_window"),
        "the ADD flow must NOT sync the old persistent window anymore"
    );
}
