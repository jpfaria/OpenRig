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

#[cfg(not(all(target_os = "linux", feature = "jack")))]
type DrumsStream = cpal::Stream;
#[cfg(all(target_os = "linux", feature = "jack"))]
type DrumsStream = crate::drums_jack_stream::DrumsJackClient;

struct DrumsStreamHandle {
    device_id: String,
    targets: Vec<usize>,
    sample_rate: u32,
    #[allow(dead_code)] // Dropping the handle is what stops the stream.
    stream: DrumsStream,
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

        if let Some(rate) = self.drums_open_on(device_id, target_channels) {
            return Ok(rate);
        }

        let host = crate::host::get_host();
        let device = crate::find_output_device_by_id(host, device_id)?
            .ok_or_else(|| anyhow::anyhow!("drums output device '{device_id}' not found"))?;
        let supported = device.default_output_config()?;
        let format = crate::aux_stream_format::aux_stream_format(
            &self.device_settings,
            device_id,
            supported.sample_rate(),
        );
        let sample_rate = format.sample_rate;
        let channels = supported.channels() as usize;
        let buffer_frames = format.buffer_frames;
        let config = crate::stream_config::build_stream_config(
            supported.channels(),
            sample_rate,
            buffer_frames,
        );

        let shared: Arc<DrumsShared> = Arc::clone(&self.drums.shared);
        let mut callback = crate::drums_callback::DrumsCallback::new(
            &shared,
            sample_rate,
            crate::aux_output_cpal::AUX_MAX_FRAMES,
        );
        let error_label = device_id.to_string();
        let targets = target_channels.to_vec();
        let callback_targets = targets.clone();
        let fader = crate::output_fader::OutputFader::of(device_id, target_channels);

        let stream = device.build_output_stream(
            &config,
            move |out: &mut [f32], _| {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    callback.fill(&shared, out, channels, &callback_targets);
                    fader.apply(out, channels, &callback_targets);
                }));
            },
            move |err| log::error!("[drums:{error_label}] output stream error: {err}"),
            None,
        )?;
        stream.play()?;

        self.swap_drums_stream(device_id, targets, sample_rate, stream);
        Ok(sample_rate)
    }

    /// The JACK build opens the drums as a client of their own on the
    /// endpoint's server; same contract as the cpal stream.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub fn start_drums(&self, device_id: &str, target_channels: &[usize]) -> Result<u32> {
        if let Some(rate) = self.drums_open_on(device_id, target_channels) {
            return Ok(rate);
        }
        let shared: Arc<DrumsShared> = Arc::clone(&self.drums.shared);
        let (client, sample_rate) =
            crate::drums_jack_stream::open_drums_jack(device_id, target_channels, &shared)?;
        self.swap_drums_stream(device_id, target_channels.to_vec(), sample_rate, client);
        Ok(sample_rate)
    }

    /// The open stream's rate when it already plays on this endpoint.
    fn drums_open_on(&self, device_id: &str, target_channels: &[usize]) -> Option<u32> {
        let stream = self.drums.stream.borrow();
        let handle = stream.as_ref()?;
        (handle.device_id == device_id && handle.targets == target_channels)
            .then_some(handle.sample_rate)
    }

    fn swap_drums_stream(
        &self,
        device_id: &str,
        targets: Vec<usize>,
        sample_rate: u32,
        stream: DrumsStream,
    ) {
        // Swapped out and dropped outside the borrow, so the old stream closes
        // with nothing else held.
        let previous = self.drums.stream.replace(Some(DrumsStreamHandle {
            device_id: device_id.to_string(),
            targets,
            sample_rate,
            stream,
        }));
        drop(previous);
    }

    /// #1081: close the drums' stream when it plays on one of `devices`, and
    /// return the endpoint to reopen it on. The kits and the groove stay.
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    pub(crate) fn release_drums_on(&self, devices: &[String]) -> Option<(String, Vec<usize>)> {
        let closed = self
            .drums
            .stream
            .borrow_mut()
            .take_if(|handle| devices.contains(&handle.device_id))?;
        Some((closed.device_id.clone(), closed.targets.clone()))
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
