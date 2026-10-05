//! Issue #1065 — audio faults must be logged at ERROR so Sentry (#1060)
//! turns them into events, with a message that stays the same across
//! occurrences (one Sentry issue per chain, not one per count).

use std::sync::Mutex;

use adapter_gui::audio_fault_log::{report_backend_lost, report_overload};

static RECORDS: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        RECORDS
            .lock()
            .unwrap()
            .push((r.level(), r.args().to_string()));
    }
    fn flush(&self) {}
}

fn errors() -> Vec<String> {
    RECORDS
        .lock()
        .unwrap()
        .iter()
        .filter(|(l, _)| *l == log::Level::Error)
        .map(|(_, m)| m.clone())
        .collect()
}

#[test]
fn issue_1065_audio_faults_log_stable_errors() {
    let _ = log::set_logger(&Capture);
    log::set_max_level(log::LevelFilter::Trace);

    report_overload("guitar", 3, 0);
    report_overload("guitar", 1, 7);
    report_backend_lost();

    let errs = errors();
    assert_eq!(errs.len(), 3, "{errs:?}");
    assert_eq!(
        errs[0], errs[1],
        "overload message must not carry the counts"
    );
    assert!(errs[0].contains("guitar"));
    assert!(errs[2].contains("backend"));
}
