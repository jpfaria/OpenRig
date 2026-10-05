//! Responsibility: writes panics into the session log synchronously (#1060).
//!
//! The async log queue may never drain when the process dies, so the
//! panic report bypasses it and goes straight to the file.

use std::io::Write;
use std::path::{Path, PathBuf};

pub fn append_panic_report(path: &Path, report: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
    {
        let _ = writeln!(f, "\n===== PANIC =====\n{report}\n=================");
        let _ = f.flush();
    }
}

/// Chains a hook that records message, location and backtrace before
/// the previous hook (stderr print) runs.
pub fn install(path: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let backtrace = std::backtrace::Backtrace::force_capture();
        let report = format!(
            "thread '{}' {info}\n{backtrace}",
            thread.name().unwrap_or("<unnamed>")
        );
        append_panic_report(&path, &report);
        previous(info);
    }));
}
