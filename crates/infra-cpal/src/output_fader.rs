//! Responsibility: scales an independent pipeline's output by the global mixer fader of its endpoint.
//!
//! A chain's routes read the global mixer fader of their endpoint; the
//! pipelines that open their own stream (metronome, backing-track player,
//! drums, DI and looper playback) read it here, so pulling a physical
//! output's fader down turns down EVERYTHING that plays there. The fader
//! glides across one callback like a route's does; an untouched fader leaves
//! the signal bit-identical. No allocation, no lock on the audio thread.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use domain::mixer_strip::MixerDirection;
use engine::mixer_gains::EndpointGain;
use engine::mixer_ramp::GainGlide;

use crate::aux_output::{AuxOutputLayout, AuxRender};

/// The global fader of one physical output endpoint, as one stream plays it.
pub(crate) struct OutputFader {
    gain: Arc<EndpointGain>,
    current_bits: AtomicU32,
}

impl OutputFader {
    /// The fader of `device_id` on `channels`. Takes the mixer table lock:
    /// build time only.
    pub(crate) fn of(device_id: &str, channels: &[usize]) -> Self {
        let gain = engine::mixer_gains::endpoint_gain(MixerDirection::Output, device_id, channels);
        let current_bits = AtomicU32::new(gain.target().to_bits());
        Self { gain, current_bits }
    }

    /// The glide for one callback of `frames`; `None` at a steady unity.
    #[inline]
    pub(crate) fn glide(&self, frames: usize) -> Option<GainGlide> {
        let current = f32::from_bits(self.current_bits.load(Ordering::Relaxed));
        let target = self.gain.target();
        self.current_bits.store(target.to_bits(), Ordering::Relaxed);
        GainGlide::begin(current, target, frames)
    }

    /// Scale `targets` of the interleaved `out` (`channels` wide).
    #[inline]
    pub(crate) fn apply(&self, out: &mut [f32], channels: usize, targets: &[usize]) {
        if channels == 0 {
            return;
        }
        let Some(glide) = self.glide(out.len() / channels) else {
            return;
        };
        for (i, frame) in out.chunks_mut(channels).enumerate() {
            let gain = glide.gain_at(i);
            for &ch in targets {
                if let Some(s) = frame.get_mut(ch) {
                    *s *= gain;
                }
            }
        }
    }
}

/// `render` scaled by the fader of `device_id` on `device_channels`, applied
/// to the layout's target channels of the buffer.
pub(crate) fn faded_render(
    device_id: &str,
    device_channels: &[usize],
    layout: &AuxOutputLayout,
    mut render: AuxRender,
) -> AuxRender {
    let fader = OutputFader::of(device_id, device_channels);
    let channels = layout.channels;
    let targets = layout.targets.clone();
    Box::new(move |out: &mut [f32]| {
        render(out);
        fader.apply(out, channels, &targets);
    })
}

#[cfg(test)]
#[path = "output_fader_tests.rs"]
mod tests;
