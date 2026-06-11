//! Issue #693 — logging must NEVER block the calling thread.
//!
//! Today `main.rs` initializes `env_logger` writing synchronously to
//! stderr: every `log::info!` on the GUI thread is a blocking write
//! behind a global lock. When stderr is a slow consumer (IDE console,
//! full pipe with no reader) the UI thread stalls for as long as the
//! write does — the user-visible "every button freezes the app".
//!
//! Contract under test: with the log sink fully stuck (worst case), a
//! burst of log calls on the caller thread still returns immediately
//! (records are queued/dropped, never awaited).

use std::io::Write;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Simulates a full pipe whose reader never drains: any direct write
/// from the logging thread hangs forever.
struct StuckSink;

impl Write for StuckSink {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn issue_693_log_burst_returns_immediately_even_with_stuck_sink() {
    adapter_gui::logging::init_logging_with_target(Box::new(StuckSink));

    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let t0 = Instant::now();
        for i in 0..200 {
            log::info!("issue 693 probe line {i} — must not block the caller");
        }
        let _ = done_tx.send(t0.elapsed());
    });

    let elapsed = done_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("log calls are stuck: the logger blocks the calling thread (issue #693)");
    assert!(
        elapsed < Duration::from_millis(500),
        "200 log calls took {elapsed:?} with a stuck sink — logging must be non-blocking"
    );
}
