//! Responsibility: initialises the process-wide logger.
//! Process-wide logger initialization (issue #693).
//!
//! The GUI thread logs dozens of lines per user action, and `env_logger`
//! alone writes them synchronously to stderr behind a global lock — a
//! slow consumer (IDE console, full pipe) turns every `log::info!` into
//! a UI stall, and a log flood from any other thread makes the UI queue
//! behind it. Here every formatted record is handed to a bounded queue
//! drained by a dedicated writer thread; when the queue is full the
//! record is DROPPED (a counter line reports the gap), so the calling
//! thread never waits on the sink. Audio threads remain zero-log per
//! invariant #8 — this is backpressure insurance for every other thread.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

/// Bounded queue depth: ~4k formatted records of in-flight logging.
/// Full queue = sink slower than producers; dropping beats blocking.
const QUEUE_DEPTH: usize = 4096;

/// What the writer thread receives: a formatted line, or a request to confirm
/// that everything queued before it has reached the sink.
enum Message {
    Line(Vec<u8>),
    Flush(SyncSender<()>),
}

/// The writer thread's queue, kept for [`flush_logging`].
static WRITER: OnceLock<SyncSender<Message>> = OnceLock::new();

struct NonBlockingWriter {
    tx: SyncSender<Message>,
    dropped: Arc<AtomicU64>,
    buf: Vec<u8>,
}

impl Write for NonBlockingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(bytes);
        // Hand off complete lines only, so the writer thread emits whole
        // records even when env_logger writes a record in several chunks.
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=pos).collect();
            match self.tx.try_send(Message::Line(line)) {
                Ok(()) | Err(TrySendError::Disconnected(_)) => {}
                Err(TrySendError::Full(_)) => {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Initialize the global logger writing to the given sink, honoring
/// `RUST_LOG` with the same `info` default `main.rs` always used. Log
/// calls never block: records go through a bounded queue to a writer
/// thread; under backpressure records are dropped and accounted.
pub fn init_logging_with_target(mut sink: Box<dyn Write + Send + 'static>) {
    let (tx, rx) = sync_channel::<Message>(QUEUE_DEPTH);
    let _ = WRITER.set(tx.clone());
    let dropped = Arc::new(AtomicU64::new(0));
    let dropped_writer = Arc::clone(&dropped);

    std::thread::Builder::new()
        .name("log-writer".into())
        .spawn(move || {
            let mut reported: u64 = 0;
            while let Ok(message) = rx.recv() {
                let line = match message {
                    Message::Line(line) => line,
                    Message::Flush(done) => {
                        let _ = sink.flush();
                        let _ = done.try_send(());
                        continue;
                    }
                };
                let _ = sink.write_all(&line);
                let total = dropped_writer.load(Ordering::Relaxed);
                if total > reported {
                    let _ = writeln!(
                        sink,
                        "[log-writer] {} record(s) dropped under backpressure",
                        total - reported
                    );
                    reported = total;
                }
            }
        })
        .expect("spawn log-writer thread");

    let logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .target(env_logger::Target::Pipe(Box::new(NonBlockingWriter {
                tx,
                dropped,
                buf: Vec::new(),
            })))
            .build();
    let max_level = logger.filter();
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(max_level);
    }
}

/// Waits up to `timeout` for every record queued so far to reach the sink.
/// `true` when it did.
pub fn flush_logging(timeout: Duration) -> bool {
    let Some(writer) = WRITER.get() else {
        return true;
    };
    let deadline = Instant::now() + timeout;
    let (done_tx, done_rx) = sync_channel(1);
    let mut request = Message::Flush(done_tx);
    // The marker queues behind every line already sent; a full queue gets
    // retried until the deadline instead of blocking.
    loop {
        match writer.try_send(request) {
            Ok(()) => break,
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(back)) => {
                if Instant::now() >= deadline {
                    return false;
                }
                request = back;
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    done_rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .is_ok()
}

/// Default initialization used by the GUI binary: log to stderr.
pub fn init_logging() {
    init_logging_with_target(default_sink());
    #[cfg(target_os = "windows")]
    log_panics();
}

/// The default panic message goes to stderr, which a windowed Windows process
/// does not have, so a crash left nothing in the log file (#978).
#[cfg(target_os = "windows")]
fn log_panics() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        flush_logging(Duration::from_secs(2));
        previous(info);
    }));
}

#[cfg(not(target_os = "windows"))]
fn default_sink() -> Box<dyn Write + Send + 'static> {
    Box::new(std::io::stderr())
}

/// The GUI is a windowed process on Windows (`windows_subsystem`), so stderr
/// usually goes nowhere and every log line and startup error was lost (#978).
/// Log to `%APPDATA%\OpenRig\logs\openrig.log` as well; stderr still gets
/// every line when the user redirects it.
#[cfg(target_os = "windows")]
fn default_sink() -> Box<dyn Write + Send + 'static> {
    let path = crate::log_file::log_file_path(&infra_filesystem::user_data_root());
    match crate::log_file::open_log_file(&path) {
        Ok(file) => Box::new(crate::log_file::TeeWriter::new(std::io::stderr(), file)),
        Err(_) => Box::new(std::io::stderr()),
    }
}
