//! Issue #978: flushing the log before exit must never hang the exit, even
//! when the sink is stuck (a full pipe, a hung network drive).

use std::io::Write;
use std::time::{Duration, Instant};

struct StuckSink;

impl Write for StuckSink {
    fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn flush_gives_up_after_its_timeout_on_a_stuck_sink() {
    adapter_gui::logging::init_logging_with_target(Box::new(StuckSink));
    log::error!("this line never reaches the sink");

    let started = Instant::now();
    let flushed = adapter_gui::logging::flush_logging(Duration::from_millis(300));

    assert!(!flushed, "a stuck sink cannot be reported as flushed");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "flush must return at its timeout, took {:?}",
        started.elapsed()
    );
}
