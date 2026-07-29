//! #323 — the recorded loop's sidecar file.
//!
//! A loop is audio, so it does not belong in the project YAML: each non-empty
//! looper is written as an interleaved-stereo wav under `<project>.loops/`,
//! and the chain's `LooperConfig.audio_file` remembers the name. Reopening the
//! project reads it back and installs it as the looper's base layer.
//!
//! Everything here is plain file I/O on the control thread — never the audio
//! thread.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use domain::ids::ChainId;

/// Folder holding a project's recorded loops: `song.openrig` → `song.loops/`.
pub fn loops_dir(project_path: &Path) -> PathBuf {
    project_path.with_extension("loops")
}

/// Disk-safe file name for one looper's audio.
pub fn loop_file_name(chain: &ChainId, looper: u64) -> String {
    let slug: String = chain
        .0
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("{}-looper-{looper}.wav", slug.trim_matches('-'))
}

/// Full path of one looper's audio beside `project_path`.
pub fn loop_file_path(project_path: &Path, chain: &ChainId, looper: u64) -> PathBuf {
    loops_dir(project_path).join(loop_file_name(chain, looper))
}

/// Write one looper's mixdown next to the project. Returns the file name to
/// store in `LooperConfig.audio_file`.
pub fn write_loop_wav(
    project_path: &Path,
    chain: &ChainId,
    looper: u64,
    pcm: &[f32],
    sample_rate: u32,
) -> Result<String> {
    let dir = loops_dir(project_path);
    std::fs::create_dir_all(&dir)?;
    let name = loop_file_name(chain, looper);

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(dir.join(&name), spec)?;
    for sample in pcm {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    Ok(name)
}

/// Read a loop back, returning the interleaved-stereo samples and the rate
/// they were recorded at (the caller resamples to the live engine rate — a
/// loop recorded at 44.1 kHz must not play 9 % fast on a 48 kHz stream).
pub fn read_loop_wav(project_path: &Path, file_name: &str) -> Result<(Vec<f32>, u32)> {
    let path = loops_dir(project_path).join(file_name);
    let mut reader = hound::WavReader::open(&path)
        .map_err(|e| anyhow!("reading loop {}: {e}", path.display()))?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = f32::from(i16::MAX);
            reader
                .samples::<i16>()
                .map(|s| s.map(|v| f32::from(v) / scale))
                .collect::<Result<_, _>>()?
        }
    };
    // A mono file is broadcast to both channels (invariant #5).
    let interleaved = if spec.channels == 1 {
        samples.iter().flat_map(|s| [*s, *s]).collect()
    } else {
        samples
    };
    Ok((interleaved, spec.sample_rate))
}

/// Linear resample of an interleaved-stereo loop between two rates. Runs off
/// the audio thread, at load time.
pub fn resample_loop(pcm: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || from_rate == 0 || pcm.is_empty() {
        return pcm.to_vec();
    }
    let src_frames = pcm.len() / 2;
    let ratio = f64::from(to_rate) / f64::from(from_rate);
    let dst_frames = ((src_frames as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(dst_frames * 2);
    for frame in 0..dst_frames {
        let pos = frame as f64 / ratio;
        let i0 = pos.floor() as usize;
        let i1 = (i0 + 1).min(src_frames.saturating_sub(1));
        let frac = (pos - pos.floor()) as f32;
        for ch in 0..2 {
            let a = pcm[i0 * 2 + ch];
            let b = pcm[i1 * 2 + ch];
            out.push(a + (b - a) * frac);
        }
    }
    out
}

#[cfg(test)]
#[path = "looper_audio_tests.rs"]
mod tests;
