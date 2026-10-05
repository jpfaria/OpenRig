//! Responsibility: fills one player output buffer from the rendered ring on the audio thread.
//!
//! The callback only copies frames and ramps the level: no decode, no stretch,
//! no allocation, no lock. Every level change (play, pause, volume, flush,
//! underrun) is a short linear ramp, so the player never clicks.

use crate::spsc::SpscRing;

use super::shared::PlayerShared;

/// Frames a full-scale level change takes (about 5 ms at 48 kHz).
pub const PLAYER_FADE_FRAMES: f32 = 256.0;

/// Interleaved stereo samples the ring holds; a power of two.
pub const PLAYER_RING_SAMPLES: usize = 16_384;

/// Callback-owned state that survives between buffers.
#[derive(Debug, Default)]
pub struct PlayerOutputState {
    gain: f32,
}

impl PlayerOutputState {
    pub fn gain(&self) -> f32 {
        self.gain
    }
}

/// Fills `out` (interleaved, `channels` wide) with the player's stereo signal
/// routed to `targets`.
///
/// Two or more targets in range take L and R; one takes the mono sum; none
/// falls back to the first two channels of the device.
pub fn fill_player_buffer(
    state: &mut PlayerOutputState,
    shared: &PlayerShared,
    ring: &SpscRing<f32>,
    out: &mut [f32],
    channels: usize,
    targets: &[usize],
) {
    out.fill(0.0);
    if channels == 0 {
        return;
    }
    let flush = shared.flush_pending();
    let target = if shared.is_playing() && flush.is_none() {
        shared.volume()
    } else {
        0.0
    };
    let step = 1.0 / PLAYER_FADE_FRAMES;
    let route = Route::resolve(channels, targets);
    let mut popped = 0u64;
    for frame in out.chunks_exact_mut(channels) {
        if target <= 0.0 && state.gain <= 0.0 {
            break;
        }
        if ring.len() < 2 {
            state.gain = 0.0;
            break;
        }
        let left = ring.pop().unwrap_or(0.0);
        let right = ring.pop().unwrap_or(0.0);
        popped += 1;
        state.gain = ramp(state.gain, target, step);
        route.write(frame, left * state.gain, right * state.gain);
    }
    if let Some(epoch) = flush {
        if state.gain <= 0.0 {
            while ring.len() >= 2 {
                ring.pop();
                ring.pop();
                popped += 1;
            }
            shared.ack_flush(epoch);
        }
    }
    if popped > 0 {
        shared.add_consumed(popped);
    }
}

fn ramp(current: f32, target: f32, step: f32) -> f32 {
    if current < target {
        (current + step).min(target)
    } else {
        (current - step).max(target)
    }
}

enum Route {
    Stereo(usize, usize),
    Mono(usize),
}

impl Route {
    fn resolve(channels: usize, targets: &[usize]) -> Self {
        let mut in_range = targets.iter().copied().filter(|&t| t < channels);
        match (in_range.next(), in_range.next()) {
            (Some(left), Some(right)) => Route::Stereo(left, right),
            (Some(only), None) => Route::Mono(only),
            _ if channels >= 2 => Route::Stereo(0, 1),
            _ => Route::Mono(0),
        }
    }

    fn write(&self, frame: &mut [f32], left: f32, right: f32) {
        match *self {
            Route::Stereo(l, r) => {
                frame[l] = left;
                frame[r] = right;
            }
            Route::Mono(c) => frame[c] = (left + right) * 0.5,
        }
    }
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
