//! Responsibility: describes the WAV data the render driver moves around.

/// On-disk sample format for the rendered output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitDepth {
    Bits16,
    Bits24,
    Bits32Float,
}

/// In-memory WAV payload: interleaved `f32` samples normalized to `[-1.0, 1.0]`.
#[derive(Debug, Clone, PartialEq)]
pub struct WavData {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

/// Errors raised by [`read_wav`] / [`write_wav_stereo`].
#[derive(Debug)]
pub enum WavError {
    Io(std::io::Error),
    Format(hound::Error),
}

impl std::fmt::Display for WavError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "wav io error: {e}"),
            Self::Format(e) => write!(f, "wav format error: {e}"),
        }
    }
}

impl std::error::Error for WavError {}

impl From<std::io::Error> for WavError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<hound::Error> for WavError {
    fn from(e: hound::Error) -> Self {
        match e {
            hound::Error::IoError(io) => Self::Io(io),
            other => Self::Format(other),
        }
    }
}
