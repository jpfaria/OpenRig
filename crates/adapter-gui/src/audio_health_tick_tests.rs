//! #913 — the audio-health tick's state machine.
//!
//! The timer runs every 2 s for the whole life of the app, so the important
//! property is restraint: announce a disconnect ONCE, retry quietly, announce
//! the recovery once. A tick that re-announced would bury the screen in toasts
//! while a JACK server was down.

use super::{health_tick, HealthReport};

fn nothing() -> HealthReport {
    HealthReport::default()
}

fn disconnected_only() -> HealthReport {
    HealthReport {
        announce_disconnect: true,
        announce_reconnect: false,
    }
}
use application::live_source::{AudioHealthReading, LiveSource};
use application::runtime_control::RuntimeControl;
use std::cell::RefCell;

struct Health(Option<(bool, bool)>);
impl LiveSource for Health {
    fn audio_health(&self) -> Option<AudioHealthReading> {
        self.0
            .map(|(running, healthy)| AudioHealthReading { running, healthy })
    }
}

#[derive(Default)]
struct Reconnect {
    result: Option<bool>,
    fails: bool,
    attempts: RefCell<usize>,
}
impl RuntimeControl for Reconnect {
    fn reconnect_audio(&self) -> anyhow::Result<bool> {
        *self.attempts.borrow_mut() += 1;
        if self.fails {
            return Err(anyhow::anyhow!("backend refused"));
        }
        Ok(self.result.unwrap_or(false))
    }
}

fn flag(value: bool) -> RefCell<bool> {
    RefCell::new(value)
}

#[test]
fn a_frontend_that_hosts_no_audio_reports_nothing() {
    let state = flag(false);
    assert_eq!(
        health_tick(&Health(None), &Reconnect::default(), &state),
        nothing()
    );
}

#[test]
fn a_stopped_rig_is_never_reported_as_disconnected() {
    // Closing a project stops every stream; that is not a backend failure.
    let state = flag(false);
    let control = Reconnect::default();
    assert_eq!(
        health_tick(&Health(Some((false, false))), &control, &state),
        nothing()
    );
    assert_eq!(
        *control.attempts.borrow(),
        0,
        "nothing to reconnect when nothing is running"
    );
}

#[test]
fn a_healthy_running_rig_reports_nothing() {
    let state = flag(false);
    assert_eq!(
        health_tick(&Health(Some((true, true))), &Reconnect::default(), &state),
        nothing()
    );
}

#[test]
fn the_first_unhealthy_tick_announces_the_disconnect() {
    let state = flag(false);
    assert_eq!(
        health_tick(&Health(Some((true, false))), &Reconnect::default(), &state),
        disconnected_only()
    );
    assert!(*state.borrow(), "the tick remembers it announced");
}

#[test]
fn the_following_unhealthy_ticks_retry_without_announcing_again() {
    let state = flag(false);
    let control = Reconnect::default();
    let unhealthy = Health(Some((true, false)));
    assert_eq!(
        health_tick(&unhealthy, &control, &state),
        disconnected_only()
    );
    for _ in 0..3 {
        assert_eq!(
            health_tick(&unhealthy, &control, &state),
            nothing(),
            "a JACK server down for a minute must not paint 30 toasts"
        );
    }
    assert_eq!(*control.attempts.borrow(), 4, "it keeps trying, quietly");
}

#[test]
fn a_successful_reconnect_announces_the_recovery_and_clears_the_flag() {
    let state = flag(true);
    let control = Reconnect {
        result: Some(true),
        ..Default::default()
    };
    assert_eq!(
        health_tick(&Health(Some((true, false))), &control, &state),
        HealthReport {
            announce_disconnect: false,
            announce_reconnect: true,
        }
    );
    assert!(!*state.borrow());
}

#[test]
fn a_reconnect_that_errors_is_treated_like_one_that_is_not_ready() {
    let state = flag(false);
    let control = Reconnect {
        fails: true,
        ..Default::default()
    };
    let unhealthy = Health(Some((true, false)));
    assert_eq!(
        health_tick(&unhealthy, &control, &state),
        disconnected_only()
    );
    assert_eq!(health_tick(&unhealthy, &control, &state), nothing());
}

#[test]
fn recovering_on_its_own_lets_the_next_failure_announce_again() {
    let state = flag(false);
    let control = Reconnect::default();
    // Down, announced.
    assert_eq!(
        health_tick(&Health(Some((true, false))), &control, &state),
        disconnected_only()
    );
    // Back up without our help — the flag clears.
    assert_eq!(
        health_tick(&Health(Some((true, true))), &control, &state),
        nothing()
    );
    assert!(!*state.borrow());
    // A second, later failure is a NEW event and is announced.
    assert_eq!(
        health_tick(&Health(Some((true, false))), &control, &state),
        disconnected_only()
    );
}
