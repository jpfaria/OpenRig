//! #967 — a finished off-thread build goes live within a few milliseconds.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use application::runtime_control::RuntimeControl;

#[derive(Default)]
struct Worker {
    installs: RefCell<usize>,
}
impl RuntimeControl for Worker {
    fn apply_finished_rebuilds(&self) -> usize {
        *self.installs.borrow_mut() += 1;
        0
    }
}

/// A scene switch builds its runtime in a few ms; the install that makes it
/// audible must follow within ~10 ms, not on a 200 ms housekeeping tick.
#[test]
fn a_finished_build_is_looked_for_within_ten_milliseconds() {
    i_slint_backend_testing::init_no_event_loop();
    let worker = Rc::new(Worker::default());
    let _timer = super::start(worker.clone());

    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(10));

    assert!(
        *worker.installs.borrow() >= 1,
        "#967: 10 ms after a build lands it must have been installed — the \
         owner heard every scene switch up to 200 ms late"
    );
}
