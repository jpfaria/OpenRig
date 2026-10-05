//! Responsibility: holds a backing track as interleaved stereo frames at one rate.
//!
//! The player is always stereo inside: a mono file is broadcast to both sides
//! and a file with more than two channels keeps its first two. That shape is
//! fixed here, once, so the renderer and the output never branch on layout.

/// Raw decoder output: interleaved samples in the file's own layout and rate.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub channels: usize,
    pub sample_rate: u32,
}

/// A whole track as interleaved `[L, R]` frames at `sample_rate`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerPcm {
    samples: Vec<f32>,
    sample_rate: u32,
}

impl PlayerPcm {
    /// Builds stereo frames from interleaved stereo samples. A trailing half
    /// frame is dropped.
    pub fn from_stereo(mut samples: Vec<f32>, sample_rate: u32) -> Self {
        samples.truncate(samples.len() - samples.len() % 2);
        Self {
            samples,
            sample_rate,
        }
    }

    /// Converts decoder output to stereo frames.
    pub fn from_decoded(decoded: DecodedAudio) -> Result<Self, String> {
        let DecodedAudio {
            samples,
            channels,
            sample_rate,
        } = decoded;
        if channels == 0 {
            return Err("the track has no audio channels".into());
        }
        if sample_rate == 0 {
            return Err("the track has no sample rate".into());
        }
        let stereo = match channels {
            1 => samples.iter().flat_map(|&s| [s, s]).collect(),
            2 => samples,
            _ => samples
                .chunks_exact(channels)
                .flat_map(|frame| [frame[0], frame[1]])
                .collect(),
        };
        Ok(Self::from_stereo(stereo, sample_rate))
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// The `[L, R]` pair at `index`, or silence past the end.
    pub fn frame(&self, index: usize) -> [f32; 2] {
        match self.samples.get(index * 2..index * 2 + 2) {
            Some(pair) => [pair[0], pair[1]],
            None => [0.0, 0.0],
        }
    }

    pub fn duration_seconds(&self) -> f64 {
        self.frames() as f64 / self.sample_rate as f64
    }

    /// The frame index nearest to `seconds`, kept inside the track.
    pub fn frame_at(&self, seconds: f64) -> usize {
        if !seconds.is_finite() || seconds <= 0.0 {
            return 0;
        }
        ((seconds * self.sample_rate as f64).round() as usize).min(self.frames())
    }
}

#[cfg(test)]
#[path = "pcm_tests.rs"]
mod tests;
