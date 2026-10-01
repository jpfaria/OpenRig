//! #979 — the tick that restarts a chain whose input arrives stepped.
//!
//! The only cure seen on the rig was the chain toggle, so a stepped chain is
//! restarted the same way, once. If the input is still stepped afterwards the
//! chain waits for the cooldown before the next attempt: a restart every tick
//! would cut the sound every 2 s. Every chain keeps its own cooldown.

use std::cell::RefCell;

use application::live_source::LiveSource;
use application::runtime_control::RuntimeControl;

use crate::stepped_input_tick::{stepped_input_tick, SteppedRestarts, RESTART_COOLDOWN_TICKS};

#[derive(Default)]
struct Rig {
    stepped: RefCell<Vec<String>>,
    restarts: RefCell<Vec<String>>,
    fails: bool,
}

impl Rig {
    fn stepped(chains: &[&str]) -> Self {
        Self {
            stepped: RefCell::new(chains.iter().map(|c| c.to_string()).collect()),
            ..Default::default()
        }
    }
}

impl LiveSource for Rig {
    fn stepped_input_chains(&self) -> Vec<String> {
        self.stepped.borrow().clone()
    }
}

impl RuntimeControl for Rig {
    fn restart_chain_streams(&self, chain_id: &str) -> anyhow::Result<bool> {
        self.restarts.borrow_mut().push(chain_id.to_string());
        if self.fails {
            return Err(anyhow::anyhow!("device refused"));
        }
        Ok(true)
    }
}

fn tick(rig: &Rig, state: &RefCell<SteppedRestarts>) -> Vec<String> {
    stepped_input_tick(rig, rig, state)
}

#[test]
fn a_rig_without_stepped_inputs_restarts_nothing() {
    let rig = Rig::default();
    let state = RefCell::new(SteppedRestarts::default());
    assert!(tick(&rig, &state).is_empty());
    assert!(rig.restarts.borrow().is_empty());
}

#[test]
fn a_stepped_chain_is_restarted() {
    let rig = Rig::stepped(&["rig:input-1"]);
    let state = RefCell::new(SteppedRestarts::default());
    assert_eq!(tick(&rig, &state), vec!["rig:input-1".to_string()]);
    assert_eq!(*rig.restarts.borrow(), vec!["rig:input-1".to_string()]);
}

#[test]
fn a_chain_still_stepped_inside_the_cooldown_is_left_alone() {
    let rig = Rig::stepped(&["rig:input-1"]);
    let state = RefCell::new(SteppedRestarts::default());
    tick(&rig, &state);
    for n in 1..RESTART_COOLDOWN_TICKS {
        assert!(
            tick(&rig, &state).is_empty(),
            "restarted again {n} ticks after the last restart"
        );
    }
    assert_eq!(rig.restarts.borrow().len(), 1);
}

#[test]
fn a_chain_still_stepped_after_the_cooldown_is_restarted_again() {
    let rig = Rig::stepped(&["rig:input-1"]);
    let state = RefCell::new(SteppedRestarts::default());
    tick(&rig, &state);
    for _ in 1..RESTART_COOLDOWN_TICKS {
        tick(&rig, &state);
    }
    assert_eq!(tick(&rig, &state), vec!["rig:input-1".to_string()]);
    assert_eq!(rig.restarts.borrow().len(), 2);
}

#[test]
fn one_chains_cooldown_never_delays_another_chain() {
    let rig = Rig::stepped(&["rig:input-1"]);
    let state = RefCell::new(SteppedRestarts::default());
    tick(&rig, &state);
    rig.stepped.borrow_mut().push("rig:input-2".to_string());
    assert_eq!(tick(&rig, &state), vec!["rig:input-2".to_string()]);
}

#[test]
fn a_failed_restart_also_waits_for_the_cooldown() {
    let rig = Rig {
        fails: true,
        ..Rig::stepped(&["rig:input-1"])
    };
    let state = RefCell::new(SteppedRestarts::default());
    assert!(
        tick(&rig, &state).is_empty(),
        "a failed restart is not reported as done"
    );
    tick(&rig, &state);
    assert_eq!(
        rig.restarts.borrow().len(),
        1,
        "a refused restart must not be retried every tick"
    );
}
