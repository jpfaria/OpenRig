//! Responsibility: writes the WAV fixtures the impulse response tests read back.

use anyhow::{Context, Result};
use std::path::Path;

pub fn write_test_stereo_ir(path: &Path) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .with_context(|| format!("failed to create test IR '{}'", path.display()))?;
    for frame in [[1.0f32, 0.5], [0.5, 1.0], [0.25, 0.25], [0.0, 0.0]] {
        writer.write_sample(frame[0])?;
        writer.write_sample(frame[1])?;
    }
    writer.finalize()?;
    Ok(())
}
