//! Issue #978: on Windows the GUI's log file is the only trace a user can send
//! back, and the queued writer (#693) could still hold the last lines, often
//! the error that ended the run, when the process exited. `flush_logging`
//! waits until everything queued so far is written.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A sink slower than the caller, like a file on a busy disk.
struct SlowSink(Arc<Mutex<Vec<u8>>>);

impl Write for SlowSink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        std::thread::sleep(Duration::from_millis(100));
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn flush_waits_for_the_last_queued_lines() {
    let written = Arc::new(Mutex::new(Vec::new()));
    adapter_gui::logging::init_logging_with_target(Box::new(SlowSink(Arc::clone(&written))));

    log::error!("openrig stopped with an error: last words");
    assert!(
        adapter_gui::logging::flush_logging(Duration::from_secs(5)),
        "a draining sink must flush within the timeout"
    );

    let text = String::from_utf8(written.lock().unwrap().clone()).unwrap();
    assert!(
        text.contains("last words"),
        "the line logged right before exit must be in the log: {text:?}"
    );
}
