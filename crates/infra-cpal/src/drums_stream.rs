//! Responsibility: runs the drum machine's own output stream.
//!
//! The drums never join a chain: they open their own output on the endpoint
//! the user picked and the backend sums them with whatever else that device
//! plays. A chain rebuild cannot chop the groove, and the groove can never
//! reach a guitar's buffers.

use std::cell::RefCell;
use std::sync::Arc;

use anyhow::Result;

use engine::drum_state::{DrumsCell, DrumsShared};

use crate::ProjectRuntimeController;

/// The drums' stream slot plus the state that outlives it, so settings and
/// the chosen kit survive a stop and a re-open.
#[derive(Default)]
pub(crate) struct DrumsHost {
    stream: RefCell<Option<DrumsStreamHandle>>,
    shared: DrumsCell,
}

// The JACK build never opens this stream, so the handle is never built there.
#[cfg_attr(all(target_os = "linux", feature = "jack"), allow(dead_code))]
struct DrumsStreamHandle {
    device_id: String,
    targets: Vec<usize>,
    sample_rate: u32,
    #[allow(dead_code)] // Dropping the handle is what stops the stream.
    stream: cpal::Stream,
}

impl ProjectRuntimeController {
    /// The drums' shared state, for the control side.
    pub fn drums_shared(&self) -> DrumsCell {
        Arc::clone(&self.drums.shared)
    }

    /// Whether the drums' stream is open.
    pub fn drums_active(&self) -> bool {
        self.drums.stream.borrow().is_some()
    }

    /// The rate the open stream runs at; kits must be loaded at it.
    pub fn drums_sample_rate(&self) -> Option<u32> {
        self.drums.stream.borrow().as_ref().map(|h| h.sample_rate)
    }

    /// Open the drums' output on `device_id`, routed to `target_channels`, and
    /// return its sample rate. Re-opening on the same endpoint is a no-op. A
    /// stream already playing is replaced only once the new one plays, so an
    /// output change that fails leaves the groove going.
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    pub fn start_drums(&self, device_id: &str, target_channels: &[usize]) -> Result<u32> {
        use cpal::traits::{DeviceTrait, StreamTrait};

        if let Some(handle) = self.drums.stream.borrow().as_ref() {
            if handle.device_id == device_id && handle.targets == target_channels {
                return Ok(handle.sample_rate);
            }
        }

        let host = crate::host::get_host();
        let device = crate::find_output_device_by_id(host, device_id)?
            .ok_or_else(|| anyhow::anyhow!("drums output device '{device_id}' not found"))?;
        let supported = device.default_output_config()?;
        let sample_rate = supported.sample_rate();
        let channels = supported.channels() as usize;
        let buffer_frames = 512u32;
        let config = crate::stream_config::build_stream_config(
            supported.channels(),
            sample_rate,
            buffer_frames,
        );

        let shared: Arc<DrumsShared> = Arc::clone(&self.drums.shared);
        let mut callback =
            crate::drums_callback::DrumsCallback::new(&shared, sample_rate, buffer_frames as usize);
        let error_label = device_id.to_string();
        let targets = target_channels.to_vec();
        let callback_targets = targets.clone();

        let stream = device.build_output_stream(
            &config,
            move |out: &mut [f32], _| {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    callback.fill(&shared, out, channels, &callback_targets);
                }));
            },
            move |err| log::error!("[drums:{error_label}] output stream error: {err}"),
            None,
        )?;
        stream.play()?;

        // Swapped out and dropped outside the borrow, so the old stream closes
        // with nothing else held.
        let previous = self.drums.stream.replace(Some(DrumsStreamHandle {
            device_id: device_id.to_string(),
            targets,
            sample_rate,
            stream,
        }));
        drop(previous);
        Ok(sample_rate)
    }

    /// The JACK build (Orange Pi) has no dedicated drums stream yet.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub fn start_drums(&self, _device_id: &str, _target_channels: &[usize]) -> Result<u32> {
        anyhow::bail!("the drum machine has no output on the JACK backend yet")
    }

    /// Close the drums' stream. Dropping the handle stops it.
    pub fn stop_drums(&self) {
        let previous = self.drums.stream.borrow_mut().take();
        drop(previous);
        // The callback is gone: nothing else holds the kits it replaced.
        self.drums.shared.kits().collect();
        self.drums.shared.grooves().collect();
    }
}
