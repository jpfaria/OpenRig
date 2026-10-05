//! Responsibility: renders one buffer of the drum machine stream.
//!
//! Runs on the audio thread: no allocation, no lock, no free. A kit or
//! groove arrives through the shared handoffs and the one it replaces is
//! dropped here only as a reference count, since the control side still
//! holds it. Everything else is read from atomics, and only when it moved.

use std::sync::Arc;

use engine::drum_state::{silent_groove, DrumKit, DrumMachine, DrumsShared};

pub(crate) struct DrumsCallback {
    machine: DrumMachine,
    sample_rate: u32,
    left: Vec<f32>,
    right: Vec<f32>,
    last_generation: u64,
    last_fill: u32,
}

impl DrumsCallback {
    /// Built on the control side, before the stream starts.
    pub(crate) fn new(shared: &DrumsShared, sample_rate: u32, buffer_frames: usize) -> Self {
        let kit = shared
            .kits()
            .latest()
            .filter(|kit| kit.sample_rate() == sample_rate)
            .unwrap_or_else(|| Arc::new(DrumKit::new("", sample_rate)));
        let groove = shared
            .grooves()
            .latest()
            .unwrap_or_else(|| Arc::new(silent_groove()));
        // The handoffs keep these alive, so the callback never frees them.
        shared.kits().retain(Arc::clone(&kit));
        shared.grooves().retain(Arc::clone(&groove));
        // Whatever is still queued is already in hand.
        drop(shared.kits().take_latest());
        drop(shared.grooves().take_latest());
        Self {
            machine: DrumMachine::new(sample_rate as f32, kit, groove, shared.settings()),
            sample_rate,
            left: vec![0.0; buffer_frames],
            right: vec![0.0; buffer_frames],
            last_generation: shared.generation(),
            last_fill: shared.fill_requests(),
        }
    }

    /// Render into `out` (interleaved, `channels` wide): left on `targets[0]`,
    /// right on `targets[1]`, or their average when the endpoint is mono.
    pub(crate) fn fill(
        &mut self,
        shared: &DrumsShared,
        out: &mut [f32],
        channels: usize,
        targets: &[usize],
    ) {
        if channels == 0 {
            return;
        }
        if !shared.enabled() {
            // Leaving the buffer untouched would replay whatever cpal handed us.
            out.fill(0.0);
            return;
        }
        self.follow_controls(shared);

        let frames = out.len() / channels;
        if self.left.len() < frames {
            // Only grows past the configured buffer size, which the stream
            // pre-allocates, so the steady state never allocates.
            self.left.resize(frames, 0.0);
            self.right.resize(frames, 0.0);
        }
        let (left, right) = (&mut self.left[..frames], &mut self.right[..frames]);
        self.machine.render(left, right);
        route(out, channels, targets, left, right);

        shared.publish_position(self.machine.position());
    }

    fn follow_controls(&mut self, shared: &DrumsShared) {
        if let Some(kit) = shared.kits().take_latest() {
            // A kit loaded at another rate would play out of tune.
            if kit.sample_rate() == self.sample_rate {
                drop(self.machine.replace_kit(kit));
            }
        }
        if let Some(groove) = shared.grooves().take_latest() {
            drop(self.machine.replace_groove(groove));
        }
        let generation = shared.generation();
        if generation != self.last_generation {
            self.machine.set_settings(shared.settings());
            self.last_generation = generation;
        }
        let playing = shared.playing();
        if playing != self.machine.is_playing() {
            if playing {
                self.machine.play();
            } else {
                self.machine.stop();
            }
        }
        let fills = shared.fill_requests();
        if fills != self.last_fill {
            self.last_fill = fills;
            self.machine.trigger_fill();
        }
    }
}

/// Writes the stereo pair to the endpoint's channels and silence elsewhere.
/// Targets outside the device are skipped; with none in range (a stale
/// binding) the pair goes to the first two channels rather than vanishing.
fn route(out: &mut [f32], channels: usize, targets: &[usize], left: &[f32], right: &[f32]) {
    let in_range = |ch: &&usize| **ch < channels;
    let mut picked = targets.iter().filter(in_range);
    let (first, second) = match (picked.next(), picked.next()) {
        (Some(&l), Some(&r)) => (Some(l), Some(r)),
        (Some(&only), None) => (Some(only), None),
        _ if channels >= 2 => (Some(0), Some(1)),
        _ => (Some(0), None),
    };
    for ((frame, l), r) in out.chunks_mut(channels).zip(left).zip(right) {
        frame.fill(0.0);
        match (first, second) {
            (Some(lc), Some(rc)) => {
                frame[lc] = *l;
                frame[rc] = *r;
            }
            (Some(mono), None) => frame[mono] = (*l + *r) * 0.5,
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "drums_callback_tests.rs"]
mod tests;
