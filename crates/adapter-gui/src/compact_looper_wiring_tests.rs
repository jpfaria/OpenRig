//! #1022 — the compact view's Looper section talks to the main window's
//! looper wiring: its actions reach the same callbacks the chain row's looper
//! panel fires, and its rows are the main row's live loopers.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, LooperCommand};
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use domain::ids::ChainId;
use project::chain::{Chain, LooperConfig};
use project::project::Project;
use slint::{ComponentHandle, Global, Model, ModelRc, SharedString, VecModel};

use super::{mirror_looper_row, wire};
use crate::state::ProjectSession;
use crate::{
    AppWindow, CompactChainViewWindow, CompactLooper, LooperItem, LooperTake, ProjectChainItem,
};

fn windows() -> (AppWindow, CompactChainViewWindow) {
    i_slint_backend_testing::init_no_event_loop();
    let main = AppWindow::new().unwrap();
    let compact = CompactChainViewWindow::new().unwrap();
    wire(&main, &compact);
    (main, compact)
}

#[test]
fn each_looper_action_reaches_the_main_windows_looper_callback() {
    let (main, compact) = windows();
    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    macro_rules! spy {
        ($on:ident, $name:literal, |$($a:ident),*|) => {{
            let h = hits.clone();
            main.$on(move |$($a),*| h.borrow_mut().push(format!(concat!($name, "{:?}"), ($($a,)*))));
        }};
    }
    spy!(on_looper_add, "add", |ci|);
    spy!(on_looper_record, "record", |ci, uid|);
    spy!(on_looper_play_all, "play_all", |ci|);
    spy!(on_looper_mix_changed, "mix", |ci, uid, v|);
    spy!(on_looper_reverse_toggled, "reverse", |ci, uid, v|);
    spy!(on_looper_preset_picked, "preset", |ci, uid, v|);

    let actions = CompactLooper::get(&compact);
    actions.invoke_add(3);
    actions.invoke_record(3, 7);
    actions.invoke_play_all(3);
    actions.invoke_mix_changed(3, 7, 80);
    actions.invoke_reverse_toggled(3, 7, true);
    actions.invoke_preset_picked(3, 7, 2);

    assert_eq!(
        *hits.borrow(),
        vec![
            "add(3,)",
            "record(3, 7)",
            "play_all(3,)",
            "mix(3, 7, 80)",
            "reverse(3, 7, true)",
            "preset(3, 7, 2)",
        ]
    );
}

fn looper(uid: i32) -> LooperItem {
    LooperItem {
        uid,
        ..Default::default()
    }
}

#[test]
fn the_section_shows_the_main_rows_loopers_and_their_options() {
    let (_main, compact) = windows();
    let strings = |v: &[&str]| {
        ModelRc::new(VecModel::from(
            v.iter().map(|s| SharedString::from(*s)).collect::<Vec<_>>(),
        ))
    };
    let row = ProjectChainItem {
        loopers: ModelRc::new(VecModel::from(vec![looper(4), looper(9)])),
        looper_input_options: strings(&["IN 1"]),
        looper_output_options: strings(&["OUT 1-2", "OUT 3-4"]),
        looper_preset_options: strings(&["Clean"]),
        ..Default::default()
    };
    mirror_looper_row(&compact, &row);

    let shown = CompactLooper::get(&compact);
    let uids: Vec<i32> = shown.get_loopers().iter().map(|l| l.uid).collect();
    assert_eq!(uids, vec![4, 9]);
    assert_eq!(shown.get_input_options().row_count(), 1);
    assert_eq!(shown.get_output_options().row_count(), 2);
    assert_eq!(shown.get_preset_options().row_count(), 1);
}

#[derive(Default)]
struct SpyDispatcher {
    seen: RefCell<Vec<Command>>,
    selection: std::sync::Arc<std::sync::RwLock<application::SelectionState>>,
}

impl CommandDispatcher for SpyDispatcher {
    fn dispatch(&self, cmd: Command) -> anyhow::Result<Vec<Event>> {
        self.seen.borrow_mut().push(cmd);
        Ok(vec![])
    }

    fn selection_state(&self) -> std::sync::Arc<std::sync::RwLock<application::SelectionState>> {
        std::sync::Arc::clone(&self.selection)
    }
}

#[test]
fn the_compact_windows_save_take_dialog_saves_through_the_bus() {
    let (_main, compact) = windows();
    let spy = Rc::new(SpyDispatcher::default());
    let session = Rc::new(RefCell::new(Some(ProjectSession::with_dispatcher(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![Chain {
                id: ChainId("rig:in".into()),
                description: None,
                instrument: "electric_guitar".into(),
                enabled: false,
                volume: 100.0,
                io_binding_ids: vec![],
                blocks: vec![],
                di_output: None,
                loopers: vec![LooperConfig::new(1)],
                disabled_endpoints: Default::default(),
                mix: Default::default(),
            }],
            midi: None,
        },
        Rc::clone(&spy) as Rc<dyn CommandDispatcher>,
        None,
        None,
        std::path::PathBuf::from("./presets"),
    ))));
    crate::looper_take_callbacks::wire_looper_take_callbacks(&compact, &session);

    LooperTake::get(&compact).invoke_save(0, 1, "riff".into());

    assert!(matches!(
        spy.seen.borrow().last(),
        Some(Command::Looper(LooperCommand::SaveChainLooperTake {
            looper: 1,
            ..
        }))
    ));
    assert_eq!(LooperTake::get(&compact).get_status(), 1, "saved");
}
