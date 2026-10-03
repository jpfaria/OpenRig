//! Responsibility: runs the metronome's own output stream.
//!
//! The stream is an auxiliary output: a cpal stream, or its own JACK client on
//! JACK builds, so the click sounds on every backend.
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

use anyhow::Result;

use engine::metronome_state::{MetronomeGenerator, MetronomeSettings, MetronomeShared};

use crate::aux_output::open_aux_output;
use crate::ProjectRuntimeController;

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
    targets: &[usize],
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

    // Route the click only to the endpoint's channels — the project's configured
    // output — so it lands where the guitar's output does and nowhere else. A
    // target beyond the device's channel count is skipped; if none are in range
    // (a stale binding) the click goes to every channel rather than vanishing.
    let any_in_range = targets.iter().any(|&ch| ch < channels);
    for (frame, click) in out.chunks_mut(channels).zip(mono.iter()) {
        if any_in_range {
            frame.fill(0.0);
            for &ch in targets {
                if let Some(s) = frame.get_mut(ch) {
                    *s = *click;
                }
            }
        } else {
            frame.fill(*click);
        }
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
    ///
    /// #127: nothing is torn down until the replacement is proven open AND
    /// started. A click that is already sounding is the one the user can hear,
    /// so an output change that cannot be honoured — the picked device gone,
    /// renamed or taken by another app — leaves it playing and returns the
    /// reason. Closing first left the flags claiming a click with no stream:
    /// `refresh_metronome_output` does not clear `enabled`, so the beat lamps,
    /// `LiveSource::metronome` and `openrig://metronome` all read "playing"
    /// over silence until the app was restarted. The cost is at most one buffer
    /// where both endpoints carry the click, which only a real output change
    /// can reach.
    pub fn start_metronome(&self, device_id: &str, target_channels: &[usize]) -> Result<()> {
        if self
            .metronome_stream
            .borrow()
            .as_ref()
            .is_some_and(|h| h.serves(device_id, target_channels))
        {
            return Ok(());
        }

        let shared = std::sync::Arc::clone(&self.metronome_shared);
        let handle = open_aux_output(
            &self.device_settings,
            device_id,
            target_channels,
            "metronome",
            |layout| {
                let mut generator =
                    MetronomeGenerator::new(layout.sample_rate as f32, shared.settings());
                // Pre-allocated here, at build time — the callback only ever grows it.
                let mut scratch: Vec<f32> = vec![0.0; layout.max_frames];
                let mut last_generation = shared.generation();
                let channels = layout.channels;
                let targets = layout.targets.clone();
                Box::new(move |out: &mut [f32]| {
                    fill_metronome_buffer(
                        &mut generator,
                        &shared,
                        &mut scratch,
                        out,
                        channels,
                        &targets,
                        &mut last_generation,
                    );
                })
            },
        )?;

        // Only now does the click that was playing go: the previous handle is
        // swapped out and dropped OUTSIDE the borrow, so the stream it owns is
        // closed with nothing else held.
        let previous = self.metronome_stream.replace(Some(handle));
        drop(previous);
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
