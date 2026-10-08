//! Responsibility: reads an IR file as one mono channel at the measurement rate.

use std::path::Path;

use anyhow::{anyhow, Result};
use ir::{IrAsset, IrChannelData};

use super::synthetic_di::DI_SAMPLE_RATE;

/// Mono (stereo folds to the channel mean), resampled to 48 kHz with the
/// same resampler the runtime uses.
pub fn load_ir_mono_48k(path: &Path) -> Result<Vec<f32>> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow!("non-UTF-8 IR path: {}", path.display()))?;
    let asset = IrAsset::load_from_wav(path_str)?;
    let mono = match asset.channel_data() {
        IrChannelData::Mono(samples) => samples.clone(),
        IrChannelData::Stereo(left, right) => {
            left.iter().zip(right).map(|(l, r)| (l + r) * 0.5).collect()
        }
    };
    Ok(ir::resample_if_needed(
        mono,
        asset.sample_rate(),
        DI_SAMPLE_RATE,
        path_str,
    ))
}
