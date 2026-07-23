//! Issue #14 — the metronome's OWN cpal output stream.
//!
//! The metronome never joins a chain, a segment or another stream's callback:
//! it opens its own output on the device the user picked and the backend sums
//! it with whatever else that device is playing. That is invariant #4 applied
//! literally — a guitar rebuild, a live edit or a chain failure cannot chop the
//! click, and the click can never reach the guitar's buffers.
//!
//! Unlike the DI (`di_stream.rs`), there is no ring and no worker thread: the
//! click is synthesized, so the callback renders it directly and the whole
//! producer side disappears.

use anyhow::{anyhow, Result};

use engine::metronome_state::{MetronomeGenerator, MetronomeSettings, MetronomeShared};

use crate::ProjectRuntimeController;

/// A live metronome stream, plus the device it was opened on so a device
/// change can tell "already running there" from "must reopen".
pub(crate) struct MetronomeStreamHandle {
    pub(crate) device_id: String,
    #[allow(dead_code)] // Dropping the handle is what stops the stream.
    stream: cpal::Stream,
}

/// Render one callback's worth of metronome into `out` (interleaved,
/// `channels` wide).
///
/// The click is mono and every channel gets the same signal — undivided,
/// because a metronome is a cue, not a stereo image.
///
/// `last_generation` is the callback's own copy of the settings version: the
/// settings are only re-read when the control side actually changed something,
/// which on the overwhelming majority of buffers is never.
pub(crate) fn fill_metronome_buffer(
    generator: &mut MetronomeGenerator,
    shared: &MetronomeShared,
    scratch: &mut Vec<f32>,
    out: &mut [f32],
    channels: usize,
    last_generation: &mut u64,
) {
    if channels == 0 {
        return;
    }
    if !shared.enabled() {
        // Leaving the buffer untouched would replay whatever cpal handed us.
        out.fill(0.0);
        return;
    }

    let generation = shared.generation();
    if generation != *last_generation {
        generator.apply(shared.settings());
        *last_generation = generation;
    }
    if shared.take_restart() {
        generator.restart();
    }

    let frames = out.len() / channels;
    if scratch.len() < frames {
        // Only ever grows, and the stream pre-allocates the configured buffer
        // size, so the steady-state callback never allocates (invariant #8).
        scratch.resize(frames, 0.0);
    }
    let mono = &mut scratch[..frames];
    generator.render(mono);

    for (frame, click) in out.chunks_mut(channels).zip(mono.iter()) {
        frame.fill(*click);
    }

    shared.publish_position(generator.position());
}

impl ProjectRuntimeController {
    /// The metronome's shared state, for the dispatcher and the UI.
    pub fn metronome_shared(&self) -> engine::metronome_state::MetronomeCell {
        std::sync::Arc::clone(&self.metronome_shared)
    }

    /// Whether the metronome's stream is open.
    pub fn metronome_active(&self) -> bool {
        self.metronome_stream.borrow().is_some()
    }

    /// Open the metronome's own output stream on `device_id`. Re-opening on the
    /// device already in use is a no-op, so a settings change never restarts a
    /// running click.
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    pub fn start_metronome(&self, device_id: &str) -> Result<()> {
        use cpal::traits::{DeviceTrait, StreamTrait};

        if self
            .metronome_stream
            .borrow()
            .as_ref()
            .is_some_and(|h| h.device_id == device_id)
        {
            return Ok(());
        }
        self.stop_metronome();

        let host = crate::host::get_host();
        let device = crate::find_output_device_by_id(host, device_id)?
            .ok_or_else(|| anyhow!("metronome output device '{device_id}' not found"))?;
        let supported = device.default_output_config()?;
        let sample_rate = supported.sample_rate();
        let channels = supported.channels() as usize;
        let buffer_frames = 512u32;
        let config = crate::stream_config::build_stream_config(
            supported.channels(),
            sample_rate,
            buffer_frames,
        );

        let shared = std::sync::Arc::clone(&self.metronome_shared);
        let mut generator =
            MetronomeGenerator::new(sample_rate as f32, self.metronome_shared.settings());
        // Pre-allocated here, at build time — the callback only ever grows it.
        let mut scratch: Vec<f32> = vec![0.0; buffer_frames as usize];
        let mut last_generation = shared.generation();
        let error_label = device_id.to_string();

        let stream = device.build_output_stream(
            &config,
            move |out: &mut [f32], _| {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    fill_metronome_buffer(
                        &mut generator,
                        &shared,
                        &mut scratch,
                        out,
                        channels,
                        &mut last_generation,
                    );
                }));
            },
            move |err| log::error!("[metronome:{error_label}] output stream error: {err}"),
            None,
        )?;
        stream.play()?;

        *self.metronome_stream.borrow_mut() = Some(MetronomeStreamHandle {
            device_id: device_id.to_string(),
            stream,
        });
        Ok(())
    }

    /// JACK build (Orange Pi) does not open a dedicated cpal stream; the
    /// metronome stays silent there until the JACK path is wired.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub fn start_metronome(&self, _device_id: &str) -> Result<()> {
        Ok(())
    }

    /// Close the metronome's stream. Dropping the handle stops it.
    pub fn stop_metronome(&self) {
        self.metronome_stream.borrow_mut().take();
    }

    /// Push new settings to the running callback (and to the next one opened).
    pub fn set_metronome_settings(&self, settings: MetronomeSettings) {
        self.metronome_shared.set_settings(settings);
    }
}

#[cfg(test)]
#[path = "metronome_stream_tests.rs"]
mod tests;
