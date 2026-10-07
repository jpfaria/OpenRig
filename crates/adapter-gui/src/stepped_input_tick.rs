//! Responsibility: decides which stepped chains one tick restarts.
//!
//! #979: a chain whose input arrives stepped is restarted (on cpal with every
//! OpenRig stream on its input device, #1081). When the restart does not cure
//! it, a restart on every 2 s tick
//! would cut the sound over and over, so each chain waits
//! [`RESTART_COOLDOWN_TICKS`] after an attempt — successful or refused — and
//! one chain's wait never delays another chain.

use std::cell::RefCell;
use std::collections::HashMap;

use application::live_source::LiveSource;
use application::runtime_control::RuntimeControl;

/// Ticks a chain waits after a restart attempt: 30 s at the 2 s health tick.
pub(crate) const RESTART_COOLDOWN_TICKS: u64 = 15;

/// What the previous ticks did, per chain.
#[derive(Default)]
pub(crate) struct SteppedRestarts {
    tick: u64,
    last_attempt: HashMap<String, u64>,
}

/// Run one tick. Returns the chains restarted on it.
pub(crate) fn stepped_input_tick(
    live: &dyn LiveSource,
    control: &dyn RuntimeControl,
    state: &RefCell<SteppedRestarts>,
) -> Vec<String> {
    let mut state = state.borrow_mut();
    let now = state.tick;
    state.tick += 1;

    let mut restarted = Vec::new();
    for chain in live.stepped_input_chains() {
        let waiting = state
            .last_attempt
            .get(&chain)
            .is_some_and(|&last| now - last < RESTART_COOLDOWN_TICKS);
        if waiting {
            continue;
        }
        state.last_attempt.insert(chain.clone(), now);
        match control.restart_chain_streams(&chain) {
            Ok(true) => restarted.push(chain),
            Ok(false) => {}
            Err(e) => log::warn!("chain '{chain}': restart after stepped input failed: {e}"),
        }
    }
    restarted
}

#[cfg(test)]
#[path = "stepped_input_tick_tests.rs"]
mod tests;
