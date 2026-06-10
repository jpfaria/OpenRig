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
use std::sync::Arc;

/// Bounded queue depth: ~4k formatted records of in-flight logging.
/// Full queue = sink slower than producers; dropping beats blocking.
const QUEUE_DEPTH: usize = 4096;

struct NonBlockingWriter {
    tx: SyncSender<Vec<u8>>,
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
            match self.tx.try_send(line) {
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
    let (tx, rx) = sync_channel::<Vec<u8>>(QUEUE_DEPTH);
    let dropped = Arc::new(AtomicU64::new(0));
    let dropped_writer = Arc::clone(&dropped);

    std::thread::Builder::new()
        .name("log-writer".into())
        .spawn(move || {
            let mut reported: u64 = 0;
            while let Ok(line) = rx.recv() {
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

    let logger = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
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

/// Default initialization used by the binaries: log to stderr.
pub fn init_logging() {
    init_logging_with_target(Box::new(std::io::stderr()));
}
