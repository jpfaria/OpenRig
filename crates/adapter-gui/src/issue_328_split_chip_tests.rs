//! #328 (spec §5.4) — clicking the Split chip (strip row or compact view, both
//! go through `select-chain-block`) opens the split editor.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Global, Timer, VecModel};

use crate::chain_graph_fixtures_tests::{mix_chain, session_with};
use crate::select_chain_block_callback::{wire, SelectChainBlockCallbackCtx};

#[test]
fn clicking_the_split_chip_opens_the_split_editor() {
    i_slint_backend_testing::init_no_event_loop();
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let window = crate::AppWindow::new().unwrap();
    let insert_window = crate::ChainInsertWindow::new().unwrap();
    let port_window = crate::ChainPortWindow::new().unwrap();
    wire(
        &window,
        &insert_window,
        &port_window,
        SelectChainBlockCallbackCtx {
            open_compact_window: Rc::new(RefCell::new(None)),
            inline_tab_state: Rc::new(RefCell::new(Default::default())),
            selected_block: Rc::new(RefCell::new(None)),
            block_editor_draft: Rc::new(RefCell::new(None)),
            insert_draft: Rc::new(RefCell::new(None)),
            block_type_options: Rc::new(VecModel::default()),
            block_model_options: Rc::new(VecModel::default()),
            filtered_block_model_options: Rc::new(VecModel::default()),
            block_model_option_labels: Rc::new(VecModel::default()),
            block_parameter_items: Rc::new(VecModel::default()),
            multi_slider_points: Rc::new(VecModel::default()),
            curve_editor_points: Rc::new(VecModel::default()),
            eq_band_curves: Rc::new(VecModel::default()),
            project_session: session_with(vec![mix_chain()]),
            project_chains: Rc::new(VecModel::default()),
            saved_project_snapshot: Rc::new(RefCell::new(None)),
            project_dirty: Rc::new(RefCell::new(false)),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            open_block_windows: Rc::new(RefCell::new(Vec::new())),
            inline_stream_timer: Rc::new(RefCell::new(None)),
            toast_timer: Rc::new(Timer::default()),
            plugin_info_window: Rc::new(RefCell::new(None)),
            block_stream_reads: Rc::new(application::live_source::NoLiveSource),
            port_draft: Rc::new(RefCell::new(None)),
        },
    );
    let opened: Rc<RefCell<Option<(i32, i32)>>> = Rc::new(RefCell::new(None));
    let seen = opened.clone();
    crate::ChainGraphOverlayState::get(&window)
        .on_open_split_editor(move |ci, kind| *seen.borrow_mut() = Some((ci, kind)));

    window.invoke_select_chain_block(0, 1); // mix_chain: [pre, sp, post]

    assert_eq!(
        *opened.borrow(),
        Some((0, 0)),
        "the split editor (kind 0) opened for chain 0"
    );
}
