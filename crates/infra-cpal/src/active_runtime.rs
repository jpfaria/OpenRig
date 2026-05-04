//! `ActiveChainRuntime` — the per-chain bundle of cpal `Stream`s plus, on
//! Linux+JACK, the live JACK `AsyncClient` and DSP worker thread handle.
//!
//! Dropping an `ActiveChainRuntime` tears down audio for that chain in
//! the right order: cpal `Stream`s and the JACK client first (callbacks
//! stop), then the worker thread via `DspWorkerHandle::drop` (sets the
//! stop flag, wakes the worker, joins). The set-and-join pattern is
//! mandatory — the worker thread is parked on a `Condvar` and would
//! otherwise miss the shutdown.
//!
//! `set_live_buffer_size` is the soft-reconfig path the supervisor uses
//! for buffer-only deltas: ask jackd to resize via the already-connected
//! live client, no SIGTERM, no libjack state corruption (issue #294 /
//! #308).

#[cfg(all(target_os = "linux", feature = "jack"))]
use anyhow::{anyhow, bail, Result};
use cpal::Stream;

use crate::resolved::ChainStreamSignature;

#[cfg(all(target_os = "linux", feature = "jack"))]
use std::sync::Arc;

#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::jack_handlers::{JackProcessHandler, JackShutdownHandler};

pub(crate) struct ActiveChainRuntime {
    // Kept for diagnostics only — issue #294 removed the signature-based
    // soft-reconfig path because it silently broke audio flow. If future
    // work reintroduces a soft-reconfig fast path, this field is the
    // natural place to compare against to decide whether a rebuild is
    // needed.
    #[allow(dead_code)]
    pub(crate) stream_signature: ChainStreamSignature,
    pub(crate) _input_streams: Vec<Stream>,
    pub(crate) _output_streams: Vec<Stream>,
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub(crate) _jack_client: Option<jack::AsyncClient<JackShutdownHandler, JackProcessHandler>>,
    /// DSP worker thread handle (Linux/JACK only). Dropped when chain stops.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub(crate) _dsp_worker: Option<DspWorkerHandle>,
}

#[cfg(all(target_os = "linux", feature = "jack"))]
impl ActiveChainRuntime {
    /// Ask jackd to resize its buffer via the already-connected live client.
    /// This is the soft-reconfig path for buffer changes: no terminate, no
    /// respawn, no libjack state corruption. The JACK driver adjusts the
    /// ALSA period in place and future process callbacks start receiving a
    /// different `n_frames`.
    ///
    /// Returns `Ok(())` only when the server actually applied the resize;
    /// any error is bubbled up so the caller can fall back to a full
    /// restart path.
    pub(crate) fn set_live_buffer_size(&self, new_frames: u32) -> Result<()> {
        let Some(client) = self._jack_client.as_ref() else {
            bail!("set_live_buffer_size: chain has no active JACK client");
        };
        client
            .as_client()
            .set_buffer_size(new_frames)
            .map_err(|e| {
                anyhow!(
                    "set_live_buffer_size: jackd refused {} frames: {:?}",
                    new_frames,
                    e
                )
            })?;
        log::info!(
            "set_live_buffer_size: applied in-place on live client → {} frames",
            new_frames
        );
        Ok(())
    }
}

/// Handle to the DSP worker thread. Setting the stop flag and joining on drop.
#[cfg(all(target_os = "linux", feature = "jack"))]
pub(crate) struct DspWorkerHandle {
    pub(crate) stop_flag: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) wake: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    pub(crate) thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(all(target_os = "linux", feature = "jack"))]
impl Drop for DspWorkerHandle {
    fn drop(&mut self) {
        self.stop_flag
            .store(true, std::sync::atomic::Ordering::Release);
        // Wake the worker so it sees the stop flag
        if let Ok(mut flag) = self.wake.0.lock() {
            *flag = true;
        }
        self.wake.1.notify_one();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}
