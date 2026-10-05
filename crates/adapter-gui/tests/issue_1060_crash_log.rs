//! Issue #1060 — installed builds must leave a log on disk.
//!
//! Contracts: a session log file is created in the log dir and old
//! sessions are pruned; every log line reaches both stderr and the file;
//! a panic report is appended to the session log.

use std::io::Write;

use adapter_gui::log_file::{log_dir, open_session_log};
use adapter_gui::panic_log::append_panic_report;
use adapter_gui::tee_writer::TeeWriter;

fn tmp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("openrig-1060-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn issue_1060_session_log_is_created_and_old_ones_pruned() {
    let dir = tmp_dir("prune");
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..5 {
        std::fs::write(dir.join(format!("openrig-{i}.log")), b"old").unwrap();
    }
    let (path, _file) = open_session_log(&dir, 3).unwrap();
    assert!(path.exists());
    let logs: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".log"))
        .collect();
    assert_eq!(
        logs.len(),
        3,
        "keeps the newest 3 sessions, including the current one"
    );
    assert!(
        !dir.join("openrig-0.log").exists(),
        "oldest session is pruned"
    );
}

#[test]
fn issue_1060_log_dir_is_the_platform_log_location() {
    let dir = log_dir().expect("log dir resolves");
    #[cfg(target_os = "macos")]
    assert!(dir.ends_with("Library/Logs/OpenRig"), "{dir:?}");
    #[cfg(not(target_os = "macos"))]
    assert!(dir.ends_with("logs"), "{dir:?}");
}

#[test]
fn issue_1060_tee_writes_every_line_to_both_sinks() {
    let mut tee = TeeWriter::new(Vec::new(), Vec::new());
    tee.write_all(b"hello\n").unwrap();
    let (a, b) = tee.into_inner();
    assert_eq!(a, b"hello\n");
    assert_eq!(b, b"hello\n");
}

#[test]
fn issue_1060_panic_report_is_appended_to_the_session_log() {
    let dir = tmp_dir("panic");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("openrig-1.log");
    std::fs::write(&path, b"before\n").unwrap();
    append_panic_report(&path, "boom at main.rs:1");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("before\n"));
    assert!(text.contains("PANIC"));
    assert!(text.contains("boom at main.rs:1"));
}
