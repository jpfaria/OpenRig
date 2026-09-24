use std::io::Write;
use std::path::Path;

use super::{log_file_path, open_log_file, TeeWriter};

#[test]
fn log_file_lives_under_the_user_data_root() {
    let root = Path::new("data-root");
    assert_eq!(log_file_path(root), root.join("logs").join("openrig.log"));
}

#[test]
fn opening_creates_the_logs_folder() {
    let root = tempfile::tempdir().unwrap();
    let path = log_file_path(root.path());
    let mut file = open_log_file(&path).expect("log file opens");
    writeln!(file, "first").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\n");
}

#[test]
fn each_session_starts_a_fresh_log() {
    let root = tempfile::tempdir().unwrap();
    let path = log_file_path(root.path());
    writeln!(open_log_file(&path).unwrap(), "previous session").unwrap();
    writeln!(open_log_file(&path).unwrap(), "this session").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "this session\n");
}

#[test]
fn tee_writes_every_byte_to_both_sinks() {
    let mut tee = TeeWriter::new(Vec::new(), Vec::new());
    tee.write_all(b"line one\n").unwrap();
    tee.write_all(b"line two\n").unwrap();
    let (a, b) = tee.into_inner();
    assert_eq!(a, b"line one\nline two\n");
    assert_eq!(b, b"line one\nline two\n");
}

/// A sink that always fails, like stderr in a windowed process with no console.
struct BrokenSink;

impl Write for BrokenSink {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("no console"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other("no console"))
    }
}

#[test]
fn tee_keeps_writing_the_file_when_the_console_is_gone() {
    let mut tee = TeeWriter::new(BrokenSink, Vec::new());
    tee.write_all(b"still logged\n").unwrap();
    let (_, file) = tee.into_inner();
    assert_eq!(file, b"still logged\n");
}
