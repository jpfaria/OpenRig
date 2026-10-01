//! #328 (spec §5.1) — picking "Split → Mix" in the add-block picker adds a
//! split at the picked position; a picker opened inside a split path offers
//! no Input, Output or Insert (orchestrator decision 8), but does offer a
//! split (spec §11: nesting at any depth).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Global, Model, Timer, VecModel};

use application::live_source::NoLiveSource;
use project::block::{AudioBlockKind, SplitEnd};

use crate::block_choose_type_callback::{self, BlockChooseTypeCallbackCtx};
use crate::block_insert_callbacks::{self, BlockInsertCallbacksCtx};
use crate::chain_graph_fixtures_tests::{chain, chain_in, core, mix_chain, session_with};
use crate::{AppWindow, BlockTypePickerItem};

struct Picker {
    app: AppWindow,
    session: Rc<RefCell<Option<crate::state::ProjectSession>>>,
    options: Rc<VecModel<BlockTypePickerItem>>,
}

fn picker_on(chains: Vec<project::chain::Chain>) -> Picker {
    i_slint_backend_testing::init_no_event_loop();
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let app = AppWindow::new().unwrap();
    let session = session_with(chains);
    let options: Rc<VecModel<BlockTypePickerItem>> = Rc::new(VecModel::default());
    let draft = Rc::new(RefCell::new(None));
    let rows = Rc::new(VecModel::default());
    let devices_in = Rc::new(RefCell::new(Vec::new()));
    let devices_out = Rc::new(RefCell::new(Vec::new()));
    let snapshot = Rc::new(RefCell::new(None));
    let dirty = Rc::new(RefCell::new(false));
    let selected = Rc::new(RefCell::new(None));
    let tabs = Rc::new(RefCell::new(Default::default()));
    block_insert_callbacks::wire(
        &app,
        BlockInsertCallbacksCtx {
            inline_tab_state: tabs.clone(),
            selected_block: selected.clone(),
            block_editor_draft: draft.clone(),
            block_type_options: options.clone(),
            block_model_options: Rc::new(VecModel::default()),
            filtered_block_model_options: Rc::new(VecModel::default()),
            block_model_option_labels: Rc::new(VecModel::default()),
            block_parameter_items: Rc::new(VecModel::default()),
            multi_slider_points: Rc::new(VecModel::default()),
            curve_editor_points: Rc::new(VecModel::default()),
            eq_band_curves: Rc::new(VecModel::default()),
            project_session: session.clone(),
            project_chains: rows.clone(),
            saved_project_snapshot: snapshot.clone(),
            project_dirty: dirty.clone(),
            input_chain_devices: devices_in.clone(),
            output_chain_devices: devices_out.clone(),
            block_editor_persist_timer: Rc::new(Timer::default()),
        },
    );
    let insert_window = crate::ChainInsertWindow::new().unwrap();
    let port_window = crate::ChainPortWindow::new().unwrap();
    block_choose_type_callback::wire(
        &app,
        &insert_window,
        &port_window,
        BlockChooseTypeCallbackCtx {
            inline_tab_state: tabs,
            block_editor_draft: draft,
            insert_draft: Rc::new(RefCell::new(None)),
            block_model_options: Rc::new(VecModel::default()),
            filtered_block_model_options: Rc::new(VecModel::default()),
            block_model_option_labels: Rc::new(VecModel::default()),
            block_parameter_items: Rc::new(VecModel::default()),
            multi_slider_points: Rc::new(VecModel::default()),
            curve_editor_points: Rc::new(VecModel::default()),
            eq_band_curves: Rc::new(VecModel::default()),
            project_session: session.clone(),
            project_chains: rows,
            block_stream_reads: Rc::new(NoLiveSource),
            saved_project_snapshot: snapshot,
            project_dirty: dirty,
            input_chain_devices: devices_in,
            output_chain_devices: devices_out,
            selected_block: selected,
            open_block_windows: Rc::new(RefCell::new(Vec::new())),
            plugin_info_window: Rc::new(RefCell::new(None)),
            port_draft: Rc::new(RefCell::new(None)),
            open_compact_window: Rc::new(RefCell::new(None)),
        },
    );
    Picker {
        app,
        session,
        options,
    }
}

fn picker() -> Picker {
    picker_on(vec![chain(vec![core("a"), core("b")])])
}

fn effect_types(options: &VecModel<BlockTypePickerItem>) -> Vec<String> {
    (0..options.row_count())
        .map(|i| options.row_data(i).unwrap().effect_type.to_string())
        .collect()
}

#[test]
fn the_picker_offers_the_split_entries_after_the_block_types() {
    let p = picker();
    p.app.invoke_start_block_insert(0, 2);
    let last: Vec<String> = (p.options.row_count() - 2..p.options.row_count())
        .map(|i| p.options.row_data(i).unwrap().label.to_string())
        .collect();
    assert_eq!(
        last,
        vec![
            rust_i18n::t!("picker-split-mix").to_string(),
            rust_i18n::t!("picker-split-y").to_string()
        ]
    );
}

#[test]
fn picking_split_to_mix_adds_a_split_at_the_position() {
    let p = picker();
    p.app.invoke_start_block_insert(0, 1);
    let mix_row = p.options.row_count() - 1; // only Mix is offered at 1 (b follows)
    crate::BlockEditorBridge::get(&p.app).invoke_choose_block_type(mix_row as i32);
    let c = chain_in(&p.session, 0);
    assert_eq!(c.blocks.len(), 3);
    assert!(
        matches!(&c.blocks[1].kind, AudioBlockKind::Split(s) if s.end == SplitEnd::Mix && s.paths.iter().all(Vec::is_empty))
    );
}

#[test]
fn a_picker_inside_a_path_offers_a_split_but_no_input_output_or_insert() {
    let p = picker_on(vec![mix_chain()]);
    crate::ChainGraphBridge::get(&p.app).invoke_start_path_insert(0, "sp".into(), 0, 0);
    let types = effect_types(&p.options);
    assert!(!types.is_empty());
    for hidden in ["input", "output", "insert"] {
        assert!(
            !types.iter().any(|t| t == hidden),
            "{hidden} offered inside a path: {types:?}"
        );
    }
    assert!(
        types.iter().any(|t| t == "split"),
        "#328 §11: a split nests inside a path: {types:?}"
    );
}

#[test]
fn a_pick_inside_a_path_maps_onto_the_filtered_rows() {
    let p = picker_on(vec![mix_chain()]);
    let before = chain_in(&p.session, 0);
    crate::ChainGraphBridge::get(&p.app).invoke_start_path_insert(0, "sp".into(), 0, 0);
    // One past the last row the path picker shows: no type, nothing happens.
    let past_end = p.options.row_count();
    crate::BlockEditorBridge::get(&p.app).invoke_choose_block_type(past_end as i32);
    assert_eq!(chain_in(&p.session, 0), before);
}
