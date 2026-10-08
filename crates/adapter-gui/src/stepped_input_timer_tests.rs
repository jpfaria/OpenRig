//! A chain whose input arrives stepped is restarted within a quarter second of
//! the trip, not on the 2 s health tick: every tick of wait is buzz the owner
//! hears.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use application::live_source::LiveSource;
use application::runtime_control::RuntimeControl;

use crate::stepped_input_tick::RESTART_COOLDOWN_TICKS;
use crate::stepped_input_timer::STEPPED_TICK;

#[derive(Default)]
struct Rig {
    stepped: Vec<String>,
    restarts: RefCell<usize>,
}

impl LiveSource for Rig {
    fn stepped_input_chains(&self) -> Vec<String> {
        self.stepped.clone()
    }
}

impl RuntimeControl for Rig {
    fn restart_chain_streams(&self, _chain_id: &str) -> anyhow::Result<bool> {
        *self.restarts.borrow_mut() += 1;
        Ok(true)
    }
}

#[test]
fn a_stepped_chain_is_restarted_within_a_quarter_second() {
    i_slint_backend_testing::init_no_event_loop();
    let rig = Rc::new(Rig {
        stepped: vec!["rig:input-4".to_string()],
        ..Default::default()
    });
    let _timer = super::start(rig.clone(), rig.clone());

    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(250));

    assert_eq!(*rig.restarts.borrow(), 1);
}

#[test]
fn the_cooldown_after_a_restart_still_lasts_thirty_seconds() {
    assert_eq!(
        STEPPED_TICK * RESTART_COOLDOWN_TICKS as u32,
        Duration::from_secs(30)
    );
}
