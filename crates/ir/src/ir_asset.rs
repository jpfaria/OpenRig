//! Responsibility: loads an impulse response out of a WAV file.

use anyhow::{bail, Context, Result};
use block_core::AudioChannelLayout;

#[derive(Debug, Clone)]
pub enum IrChannelData {
    Mono(Vec<f32>),
    Stereo(Vec<f32>, Vec<f32>),
}

#[derive(Debug, Clone)]
pub struct IrAsset {
    pub(crate) sample_rate: u32,
    pub(crate) channel_data: IrChannelData,
}

impl IrAsset {
    pub fn load_from_wav(path: &str) -> Result<Self> {
        let mut reader = hound::WavReader::open(path)
            .with_context(|| format!("failed to open IR wav '{}'", path))?;
        let spec = reader.spec();
        let channels = spec.channels as usize;
        if channels == 0 || channels > 2 {
            bail!(
                "IR '{}' uses {} channels; only mono and stereo IRs are supported",
                path,
                channels
            );
        }

        let interleaved = match spec.sample_format {
            hound::SampleFormat::Float => reader
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .with_context(|| format!("failed to read float samples from '{}'", path))?,
            hound::SampleFormat::Int => {
                let max_amplitude =
                    ((1i64 << (spec.bits_per_sample.saturating_sub(1) as u32)) - 1).max(1) as f32;
                reader
                    .samples::<i32>()
                    .map(|sample| sample.map(|value| value as f32 / max_amplitude))
                    .collect::<Result<Vec<_>, _>>()
                    .with_context(|| format!("failed to read PCM samples from '{}'", path))?
            }
        };

        if interleaved.is_empty() {
            bail!("IR '{}' contains no samples", path);
        }
        if interleaved.len() % channels != 0 {
            bail!(
                "IR '{}' sample data is not aligned to its channel count",
                path
            );
        }

        let channel_data = match channels {
            1 => IrChannelData::Mono(interleaved),
            2 => {
                let mut left = Vec::with_capacity(interleaved.len() / 2);
                let mut right = Vec::with_capacity(interleaved.len() / 2);
                for frame in interleaved.chunks_exact(2) {
                    left.push(frame[0]);
                    right.push(frame[1]);
                }
                IrChannelData::Stereo(left, right)
            }
            _ => unreachable!(),
        };

        Ok(Self {
            sample_rate: spec.sample_rate,
            channel_data,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channel_count(&self) -> usize {
        match &self.channel_data {
            IrChannelData::Mono(_) => 1,
            IrChannelData::Stereo(_, _) => 2,
        }
    }

    pub fn channel_layout(&self) -> AudioChannelLayout {
        match self.channel_count() {
            1 => AudioChannelLayout::Mono,
            2 => AudioChannelLayout::Stereo,
            _ => unreachable!(),
        }
    }

    pub fn frame_count(&self) -> usize {
        match &self.channel_data {
            IrChannelData::Mono(samples) => samples.len(),
            IrChannelData::Stereo(left, _) => left.len(),
        }
    }

    pub fn channel_data(&self) -> &IrChannelData {
        &self.channel_data
    }
}
