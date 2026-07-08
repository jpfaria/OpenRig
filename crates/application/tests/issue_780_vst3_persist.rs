//! Issue #780 — end-to-end proof that VST3 parameter values changed in the
//! native editor persist to the project. Simulates the native editor by driving
//! the plugin's controller directly (exactly what `performEdit` does), then
//! dispatches `CaptureRigEdits` (the save path) and asserts the block's params
//! captured the change as `p{id}` percent.
//!
//! Env-gated on OPENRIG_TEST_VST3_DIR; run with --test-threads=1 (JUCE plugins
//! refuse concurrent instantiation):
//!   OPENRIG_TEST_VST3_DIR=<.../vst3> cargo test -p application \
//!     --test issue_780_vst3_persist -- --test-threads=1

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use block_core::param::set::ParameterSet;
use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::project::Project;

const SR: f64 = 48_000.0;
const BLOCK_KEY: &str = "rig:gtr:block:0";

fn chow_entry() -> Option<&'static vst3_host::Vst3CatalogEntry> {
    let dir = std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from)?;
    vst3_host::init_vst3_catalog(SR, &[dir]);
    vst3_host::vst3_catalog().iter().find(|e| {
        e.info
            .bundle_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("ChowCentaur.vst3"))
            .unwrap_or(false)
    })
}

#[test]
fn native_editor_param_change_persists_via_capture_rig_edits() {
    let Some(entry) = chow_entry() else { return };
    let uid = vst3_host::resolve_uid_for_model(&entry.model_id).unwrap();
    let plugin =
        vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
    // The engine registers the block's GUI context under its BlockId; mirror it.
    let _ = vst3_host::register_vst3_gui_context(
        BLOCK_KEY,
        &entry.model_id,
        plugin.controller_clone(),
        plugin.library_arc(),
    );

    // Drive a param away from its default, like the native editor's knob does.
    let info = plugin.param_info(0).expect("has a param");
    let target = if info.default_normalized < 0.5 { 0.85 } else { 0.15 };
    plugin.set_param(info.id, target).unwrap();

    // A project whose one chain has this VST3 block (empty params, as a
    // light-scan catalog VST3 starts).
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        midi: None,
        chains: vec![Chain {
            id: ChainId("rig:gtr".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: Vec::new(),
            blocks: vec![AudioBlock {
                id: BlockId(BLOCK_KEY.into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: block_core::EFFECT_TYPE_VST3.into(),
                    model: entry.model_id.into(),
                    params: ParameterSet::default(),
                }),
            }],
            di_output: None,
        }],
    }));

    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher
        .dispatch(Command::CaptureRigEdits)
        .expect("dispatch ok");

    let borrow = project.borrow();
    let AudioBlockKind::Core(core) = &borrow.chains[0].blocks[0].kind else {
        panic!("expected core block");
    };
    let key = format!("p{}", info.id);
    let got = core
        .params
        .get_f32(&key)
        .unwrap_or_else(|| panic!("expected {key} to be persisted, params={:?}", core.params));
    assert!(
        (got - (target as f32) * 100.0).abs() < 1.0,
        "persisted {got}% want ~{}%",
        target * 100.0
    );
    drop(borrow);
    drop(plugin);
}
