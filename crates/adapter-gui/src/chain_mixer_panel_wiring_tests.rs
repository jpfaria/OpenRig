//! #1007 — the chain list's mixer overlay draws the chain whose button opened
//! it, and its MASTER fader moves THAT chain's volume.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;
use slint::{Global, Model};

use super::wire;
use crate::state::ProjectSession;
use crate::{AppWindow, ChainMixerBridge, ChainMixerPanel};

fn chain(id: &str, volume: f32) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        mix: Default::default(),
    }
}

fn session(chains: Vec<Chain>) -> Rc<RefCell<Option<ProjectSession>>> {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains,
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-1007-chain-mixer-panel-tests"),
    ))))
}

fn wired(session: &Rc<RefCell<Option<ProjectSession>>>) -> AppWindow {
    i_slint_backend_testing::init_no_event_loop();
    let w = AppWindow::new().expect("main window");
    wire(
        &w,
        session.clone(),
        Rc::new(RefCell::new(None)),
        Rc::new(RefCell::new(false)),
    );
    w
}

/// The MASTER caption on screen: the chain volume, e.g. "50%".
fn master_detail(w: &AppWindow) -> Option<String> {
    ChainMixerBridge::get(w)
        .get_master()
        .row_data(0)
        .map(|row| row.detail.to_string())
}

#[test]
fn opening_the_mixer_draws_the_chain_it_names() {
    let s = session(vec![chain("a", 100.0), chain("b", 50.0)]);
    let w = wired(&s);
    ChainMixerPanel::get(&w).invoke_opened(1);
    assert_eq!(master_detail(&w).as_deref(), Some("50%"));
    ChainMixerPanel::get(&w).invoke_opened(0);
    assert_eq!(master_detail(&w).as_deref(), Some("100%"));
}

#[test]
fn without_a_project_opening_the_mixer_draws_nothing() {
    let w = wired(&Rc::new(RefCell::new(None)));
    ChainMixerPanel::get(&w).invoke_opened(0);
    assert_eq!(master_detail(&w), None);
    assert_eq!(ChainMixerPanel::get(&w).get_global_inputs().row_count(), 0);
}

#[test]
fn the_master_fader_moves_the_volume_of_the_open_chain() {
    let s = session(vec![chain("a", 100.0), chain("b", 50.0)]);
    let w = wired(&s);
    let moved = Rc::new(RefCell::new(Vec::<i32>::new()));
    let m = moved.clone();
    w.on_chain_volume_changed(move |ci, _| m.borrow_mut().push(ci));
    ChainMixerPanel::get(&w).invoke_opened(1);
    ChainMixerBridge::get(&w).invoke_single_fader_moved("master".into(), 1.0);
    assert_eq!(*moved.borrow(), vec![1]);
}
