//! Responsibility: converts a decoded backing track to the output device rate.
//!
//! Runs once per load on the player worker, never on the audio thread: the
//! whole track is resampled up front so playback only ever copies frames.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use super::pcm::PlayerPcm;

/// Frames per FFT chunk: large enough for an efficient whole-file pass, small
/// enough that the resampler's own buffers stay a few kilobytes.
const RESAMPLE_CHUNK_FRAMES: usize = 1024;

/// The same track at `target_rate`. A track already at that rate is returned
/// as it is.
pub fn resample_to(pcm: PlayerPcm, target_rate: u32) -> Result<PlayerPcm, String> {
    if target_rate == 0 {
        return Err("the output has no sample rate".into());
    }
    if pcm.sample_rate() == target_rate {
        return Ok(pcm);
    }
    if pcm.frames() == 0 {
        return Ok(PlayerPcm::from_stereo(Vec::new(), target_rate));
    }
    let mut resampler = Fft::<f32>::new(
        pcm.sample_rate() as usize,
        target_rate as usize,
        RESAMPLE_CHUNK_FRAMES,
        2,
        FixedSync::Input,
    )
    .map_err(|error| format!("resampler: {error}"))?;
    let input = InterleavedSlice::new(pcm.samples(), 2, pcm.frames())
        .map_err(|error| format!("resampler input: {error}"))?;
    let output = resampler
        .process_all(&input, pcm.frames(), None)
        .map_err(|error| format!("resampling: {error}"))?;
    Ok(PlayerPcm::from_stereo(output.take_data(), target_rate))
}

#[cfg(test)]
#[path = "resample_tests.rs"]
mod tests;
