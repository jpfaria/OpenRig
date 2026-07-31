//! #85 — clicking a mid port already in the chain must reopen its editor.
//!
//! What the user hits instead: "This block cannot be edited from the GUI yet."
//! The port is added, shows in the strip, and is then uneditable — its E/S can
//! never be changed. This drives the REAL click callback (`on_select_chain_block`),
//! not a helper, and checks the editor's draft (#749/#761: render proves layout,
//! only an interaction test proves the click lands).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Model, Timer, VecModel};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::ChannelMode;
use infra_filesystem::{IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::project_ops::create_new_project_session;
use crate::select_chain_block_callback::{wire, SelectChainBlockCallbackCtx};
use crate::state::{PortDraft, ProjectSession};
use crate::{AppWindow, ChainInsertWindow, ChainPortWindow};

fn core(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn mid_output() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:input-1:output:1".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "Aux out".into(),
        }),
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![],
            outputs: vec![IoEndpoint {
                name: "Main out".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: vec![],
            outputs: vec![IoEndpoint {
                name: "Aux out".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        },
    ]
}

/// A session whose chain is `[gain, Output(aux), gain]` — the port is at 1.
fn session() -> ProjectSession {
    let tmp = tempfile::TempDir::new().unwrap();
    let session = create_new_project_session(&tmp.path().join("config.yaml"));
    std::mem::forget(tmp);
    session.project.borrow_mut().chains = vec![Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![core("A"), mid_output(), core("B")],
        di_output: None,
        loopers: vec![],
    }];
    *session.io_bindings.borrow_mut() = registry();
    session
}

/// What the port editor shows after the click.
struct Opened {
    draft: Option<PortDraft>,
    window: ChainPortWindow,
}

/// Clicks the block at `ui_index` through the real callback and returns the
/// port editor's draft afterwards.
fn click(ui_index: i32) -> Option<PortDraft> {
    open(ui_index).draft
}

/// Clicks the block at `ui_index` with the port window's option models wired
/// the way `run_desktop_app` wires them, so the selects are inspectable.
fn open(ui_index: i32) -> Opened {
    i_slint_backend_testing::init_no_event_loop();
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let window = AppWindow::new().unwrap();
    let insert_window = ChainInsertWindow::new().unwrap();
    let port_window = ChainPortWindow::new().unwrap();
    let port_draft: Rc<RefCell<Option<PortDraft>>> = Rc::new(RefCell::new(None));
    let binding_options = Rc::new(VecModel::<slint::SharedString>::default());
    let endpoint_options = Rc::new(VecModel::<slint::SharedString>::default());
    port_window.set_binding_options(slint::ModelRc::from(binding_options.clone()));
    port_window.set_endpoint_options(slint::ModelRc::from(endpoint_options.clone()));

    wire(
        &window,
        &insert_window,
        &port_window,
        SelectChainBlockCallbackCtx {
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
            project_session: Rc::new(RefCell::new(Some(session()))),
            project_chains: Rc::new(VecModel::default()),
            project_runtime: Rc::new(RefCell::new(None)),
            saved_project_snapshot: Rc::new(RefCell::new(None)),
            project_dirty: Rc::new(RefCell::new(false)),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            chain_input_device_options: Rc::new(VecModel::default()),
            chain_output_device_options: Rc::new(VecModel::default()),
            insert_send_channels: Rc::new(VecModel::default()),
            insert_return_channels: Rc::new(VecModel::default()),
            open_block_windows: Rc::new(RefCell::new(Vec::new())),
            inline_stream_timer: Rc::new(RefCell::new(None)),
            toast_timer: Rc::new(Timer::default()),
            plugin_info_window: Rc::new(RefCell::new(None)),
            port_draft: port_draft.clone(),
            auto_save: false,
        },
    );

    window.invoke_select_chain_block(0, ui_index);
    let draft = port_draft.borrow().clone();
    Opened {
        draft,
        window: port_window,
    }
}

#[test]
fn clicking_a_mid_output_opens_its_port_editor() {
    let draft = click(1).expect(
        "#85: clicking the mid Output must open the port editor — it errors with \
         'this block cannot be edited from the GUI yet' instead, so the port can \
         never be re-pointed after it is added",
    );

    assert_eq!(
        draft.block_index, 1,
        "the editor must target the clicked port"
    );
    assert!(!draft.is_input, "an Output port opens in output mode");
    assert_eq!(
        (draft.io.as_str(), draft.endpoint.as_str()),
        ("aux", "Aux out"),
        "the editor must open on the E/S the port already points at"
    );
}

#[test]
fn the_editor_opens_with_the_ports_endpoint_already_picked() {
    let opened = open(1);
    let endpoints: Vec<String> = opened
        .window
        .get_endpoint_options()
        .iter()
        .map(|s| s.to_string())
        .collect();

    assert_eq!(
        endpoints,
        vec!["Aux out".to_string()],
        "the ENDPOINT select must list the picked E/S's outputs"
    );
    assert_eq!(
        opened.window.get_selected_endpoint_index(),
        0,
        "#85: the ENDPOINT select shows up EMPTY even though the port already \
         points at 'Aux out' — the user cannot tell where the port goes"
    );
}

/// What each select in the port editor actually DISPLAYS, top to bottom.
fn shown_values(window: &ChainPortWindow) -> Vec<String> {
    i_slint_backend_testing::ElementHandle::find_by_element_type_name(window, "ComboBox")
        .map(|el| el.accessible_value().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn the_editor_displays_the_ports_endpoint_not_an_empty_box() {
    let opened = open(1);
    let shown = shown_values(&opened.window);

    assert_eq!(
        shown,
        vec!["AUX".to_string(), "Aux out".to_string()],
        "#85: the port points at AUX / 'Aux out', but the ENDPOINT select renders \
         EMPTY — picking the index is not enough, the box must show the value"
    );
}

#[test]
fn clicking_a_processing_block_does_not_open_the_port_editor() {
    assert!(
        click(0).is_none(),
        "a gain block has nothing to do with the port editor"
    );
}
