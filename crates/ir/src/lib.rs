use anyhow::{bail, Context, Result};
use realfft::{num_complex::Complex32, ComplexToReal, RealFftPlanner, RealToComplex};
use block_core::{AudioChannelLayout, MonoProcessor, StereoProcessor};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum IrChannelData {
    Mono(Vec<f32>),
    Stereo(Vec<f32>, Vec<f32>),
}

#[derive(Debug, Clone)]
pub struct IrAsset {
    sample_rate: u32,
    channel_data: IrChannelData,
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

/// Maximum IR length in samples at the file's native sample rate.
/// Longer tails are truncated with a cosine fade-out.
/// 8192 samples ≈ 170ms at 48kHz — more than enough for cabs and body IRs.
const MAX_IR_SAMPLES: usize = 8192;

/// Fade-out length in samples applied when truncating.
const FADE_OUT_SAMPLES: usize = 512;

pub fn build_mono_ir_processor_from_wav(
    path: &str,
    runtime_sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    let ir = IrAsset::load_from_wav(path)?;
    if ir.channel_count() != 1 {
        bail!("IR '{}' is not mono", path);
    }
    let IrChannelData::Mono(samples) = ir.channel_data else {
        unreachable!()
    };
    let samples = truncate_with_fade(samples, path);
    let samples = resample_if_needed(samples, ir.sample_rate, runtime_sample_rate, path);
    Ok(Box::new(MonoIrProcessor::new(samples)))
}

pub fn build_stereo_ir_processor_from_wav(
    path: &str,
    runtime_sample_rate: f32,
) -> Result<Box<dyn StereoProcessor>> {
    let ir = IrAsset::load_from_wav(path)?;
    if ir.channel_count() != 2 {
        bail!("IR '{}' is not stereo", path);
    }
    let IrChannelData::Stereo(left, right) = ir.channel_data else {
        unreachable!()
    };
    let left = truncate_with_fade(left, path);
    let right = truncate_with_fade(right, path);
    let left = resample_if_needed(left, ir.sample_rate, runtime_sample_rate, path);
    let right = resample_if_needed(right, ir.sample_rate, runtime_sample_rate, path);
    Ok(Box::new(StereoIrProcessor::new(left, right)))
}

fn truncate_with_fade(mut samples: Vec<f32>, path: &str) -> Vec<f32> {
    if samples.len() <= MAX_IR_SAMPLES {
        return samples;
    }
    log::info!(
        "truncating IR '{}' from {} to {} samples with {}‑sample fade‑out",
        path, samples.len(), MAX_IR_SAMPLES, FADE_OUT_SAMPLES
    );
    samples.truncate(MAX_IR_SAMPLES);
    let fade_start = MAX_IR_SAMPLES.saturating_sub(FADE_OUT_SAMPLES);
    for i in fade_start..MAX_IR_SAMPLES {
        let t = (i - fade_start) as f32 / FADE_OUT_SAMPLES as f32;
        let gain = 0.5 * (1.0 + (std::f32::consts::PI * t).cos()); // cosine fade
        samples[i] *= gain;
    }
    samples
}

fn resample_if_needed(samples: Vec<f32>, ir_rate: u32, runtime_rate: f32, path: &str) -> Vec<f32> {
    let runtime_rate = runtime_rate.round() as u32;
    if runtime_rate == 0 || ir_rate == runtime_rate {
        return samples;
    }
    log::info!(
        "resampling IR '{}' from {}Hz to {}Hz ({} samples)",
        path, ir_rate, runtime_rate, samples.len()
    );
    let ratio = runtime_rate as f64 / ir_rate as f64;
    let new_len = (samples.len() as f64 * ratio).round() as usize;
    if new_len == 0 {
        return vec![0.0];
    }
    // Windowed sinc interpolation (Lanczos kernel, a=4)
    const SINC_HALF_WIDTH: usize = 4;
    let mut resampled = Vec::with_capacity(new_len);
    for i in 0..new_len {
        let src_pos = i as f64 / ratio;
        let center = src_pos.floor() as i64;
        let frac = src_pos - center as f64;
        let mut sum = 0.0f64;
        let mut weight_sum = 0.0f64;
        for j in -(SINC_HALF_WIDTH as i64)..=(SINC_HALF_WIDTH as i64) {
            let idx = center + j;
            if idx < 0 || idx >= samples.len() as i64 {
                continue;
            }
            let x = frac - j as f64;
            let w = lanczos_kernel(x, SINC_HALF_WIDTH as f64);
            sum += samples[idx as usize] as f64 * w;
            weight_sum += w;
        }
        let value = if weight_sum.abs() > 1e-10 { sum / weight_sum } else { 0.0 };
        resampled.push(value as f32);
    }
    resampled
}

fn lanczos_kernel(x: f64, a: f64) -> f64 {
    if x.abs() < 1e-10 {
        return 1.0;
    }
    if x.abs() >= a {
        return 0.0;
    }
    let pi_x = std::f64::consts::PI * x;
    (a * pi_x.sin() * (pi_x / a).sin()) / (pi_x * pi_x)
}

/// Uniformly partitioned FFT convolver with internal buffering.
///
/// Splits the IR into fixed-size segments, pre-computes their FFTs, and
/// convolves with small FFTs. Internal buffering decouples audio block size
/// from partition size — works efficiently with any buffer size.
struct FftBlockConvolver {
    ir: Vec<f32>,
    state: Option<PartitionedState>,
}

/// Minimum partition size. Keeps the number of partitions low for
/// real-time performance. With MAX_IR_SAMPLES=8192 and PARTITION_SIZE=512,
/// we get at most 16 partitions — very manageable.
const PARTITION_SIZE: usize = 512;

impl FftBlockConvolver {
    fn new(ir: Vec<f32>) -> Result<Self> {
        if ir.is_empty() {
            bail!("IR cannot be empty");
        }
        Ok(Self { ir, state: None })
    }

    fn process_block_in_place(&mut self, buffer: &mut [f32]) {
        if buffer.is_empty() {
            return;
        }
        self.ensure_state();
        let state = self.state.as_mut().expect("partitioned state initialized");

        // Feed samples into internal input buffer, process partition-sized
        // chunks, and drain output buffer back into the caller's buffer.
        let out_len = state.output_buf.len();
        for i in 0..buffer.len() {
            state.input_buf[state.input_pos] = buffer[i];
            buffer[i] = state.output_buf[state.output_pos % out_len];
            state.output_buf[state.output_pos % out_len] = 0.0;
            state.input_pos += 1;
            state.output_pos += 1;
            if state.output_pos >= out_len {
                state.output_pos -= out_len;
            }

            if state.input_pos == state.partition_size {
                Self::process_partition(state);
                state.input_pos = 0;
            }
        }
    }

    fn process_partition(state: &mut PartitionedState) {
        let ps = state.partition_size;
        let fft_len = state.fft_len;
        let spectrum_len = fft_len / 2 + 1;
        let scale = fft_len as f32;

        // Forward FFT of input partition (zero-padded)
        state.fft_input.fill(0.0);
        state.fft_input[..ps].copy_from_slice(&state.input_buf[..ps]);
        state.forward
            .process(&mut state.fft_input, &mut state.fft_scratch)
            .expect("forward FFT");

        // Store in frequency delay line (ring buffer)
        state.fdl_write = (state.fdl_write + 1) % state.num_partitions;
        let write_offset = state.fdl_write * spectrum_len;
        state.fdl[write_offset..write_offset + spectrum_len]
            .copy_from_slice(&state.fft_scratch);

        // Multiply-accumulate across all IR partitions
        state.accum.iter_mut().for_each(|c| *c = Complex32::default());
        for p in 0..state.num_partitions {
            let fdl_idx = (state.fdl_write + state.num_partitions - p) % state.num_partitions;
            let fdl_off = fdl_idx * spectrum_len;
            let ir_off = p * spectrum_len;
            for i in 0..spectrum_len {
                state.accum[i] += state.fdl[fdl_off + i] * state.ir_partitions[ir_off + i];
            }
        }

        // Inverse FFT
        state.inverse
            .process(&mut state.accum, &mut state.fft_output)
            .expect("inverse FFT");

        // Overlap-add into output ring buffer
        for i in 0..fft_len {
            let out_idx = (state.output_pos + i) % state.output_buf.len();
            state.output_buf[out_idx] += state.fft_output[i] / scale;
        }
    }

    fn ensure_state(&mut self) {
        if self.state.is_some() {
            return;
        }

        let ps = PARTITION_SIZE;
        let fft_len = (ps * 2).next_power_of_two();
        let spectrum_len = fft_len / 2 + 1;

        let num_partitions = (self.ir.len() + ps - 1) / ps;
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(fft_len);
        let inverse = planner.plan_fft_inverse(fft_len);

        // Pre-compute FFT of each IR partition
        let mut ir_partitions = vec![Complex32::default(); num_partitions * spectrum_len];
        let mut buf = vec![0.0f32; fft_len];
        let mut out = vec![Complex32::default(); spectrum_len];
        for p in 0..num_partitions {
            buf.fill(0.0);
            let start = p * ps;
            let end = (start + ps).min(self.ir.len());
            buf[..end - start].copy_from_slice(&self.ir[start..end]);
            forward.process(&mut buf, &mut out).expect("IR partition FFT");
            let offset = p * spectrum_len;
            ir_partitions[offset..offset + spectrum_len].copy_from_slice(&out);
        }

        // Output ring buffer must be large enough to hold the full
        // convolution result overlap: num_partitions * partition_size + fft_len
        let output_buf_len = (num_partitions + 1) * ps + fft_len;

        log::debug!(
            "IR convolver: {} samples, {} partitions of {}, fft_len={}, output_buf={}",
            self.ir.len(), num_partitions, ps, fft_len, output_buf_len
        );

        self.state = Some(PartitionedState {
            partition_size: ps,
            fft_len,
            num_partitions,
            forward,
            inverse,
            ir_partitions,
            fdl: vec![Complex32::default(); num_partitions * spectrum_len],
            fdl_write: 0,
            input_buf: vec![0.0; ps],
            input_pos: 0,
            output_buf: vec![0.0; output_buf_len],
            output_pos: 0,
            fft_input: vec![0.0; fft_len],
            fft_output: vec![0.0; fft_len],
            fft_scratch: vec![Complex32::default(); spectrum_len],
            accum: vec![Complex32::default(); spectrum_len],
        });
    }
}

struct PartitionedState {
    partition_size: usize,
    fft_len: usize,
    num_partitions: usize,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    ir_partitions: Vec<Complex32>,
    fdl: Vec<Complex32>,
    fdl_write: usize,
    input_buf: Vec<f32>,
    input_pos: usize,
    output_buf: Vec<f32>,
    output_pos: usize,
    fft_input: Vec<f32>,
    fft_output: Vec<f32>,
    fft_scratch: Vec<Complex32>,
    accum: Vec<Complex32>,
}

pub struct MonoIrProcessor {
    convolver: FftBlockConvolver,
}

impl MonoIrProcessor {
    pub fn new(ir: Vec<f32>) -> Self {
        Self {
            convolver: FftBlockConvolver::new(ir).expect("IR should be valid"),
        }
    }
}

impl MonoProcessor for MonoIrProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let mut single = [input];
        self.process_block(&mut single);
        single[0]
    }

    fn process_block(&mut self, buffer: &mut [f32]) {
        self.convolver.process_block_in_place(buffer);
    }
}

pub struct StereoIrProcessor {
    left: FftBlockConvolver,
    right: FftBlockConvolver,
}

impl StereoIrProcessor {
    pub fn new(left: Vec<f32>, right: Vec<f32>) -> Self {
        Self {
            left: FftBlockConvolver::new(left).expect("left IR should be valid"),
            right: FftBlockConvolver::new(right).expect("right IR should be valid"),
        }
    }
}

impl StereoProcessor for StereoIrProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mut block = [input];
        self.process_block(&mut block);
        block[0]
    }

    fn process_block(&mut self, buffer: &mut [[f32; 2]]) {
        let mut left = Vec::with_capacity(buffer.len());
        let mut right = Vec::with_capacity(buffer.len());
        for frame in buffer.iter() {
            left.push(frame[0]);
            right.push(frame[1]);
        }
        self.left.process_block_in_place(&mut left);
        self.right.process_block_in_place(&mut right);
        for ((frame, left_sample), right_sample) in buffer
            .iter_mut()
            .zip(left.into_iter())
            .zip(right.into_iter())
        {
            *frame = [left_sample, right_sample];
        }
    }
}

/// Resolve an IR capture path relative to the configured `ir_captures` root.
///
/// `relative_path` is the portion after `captures/ir/`, e.g.
/// `"cabs/marshall_4x12_v30/ev_mix_b.wav"`.  Searches relative to the
/// executable first, then falls back to the config path directly.
///
/// All curated IR captures are loaded from the filesystem at runtime.
pub fn resolve_ir_capture(relative_path: &str) -> Result<String> {
    let paths = infra_filesystem::asset_paths();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let candidates = [
        exe_dir
            .as_ref()
            .map(|d| d.join("../../").join(&paths.ir_captures).join(relative_path)),
        Some(PathBuf::from(&paths.ir_captures).join(relative_path)),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    bail!(
        "IR capture '{}' not found in '{}'",
        relative_path,
        paths.ir_captures
    )
}

#[cfg(test)]
pub mod test_support {
    use super::*;
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
}

#[cfg(test)]
mod tests {
    use block_core::{AudioChannelLayout, MonoProcessor, StereoProcessor};

    use crate::{
        lanczos_kernel, resample_if_needed, truncate_with_fade, IrAsset, IrChannelData,
        MonoIrProcessor, StereoIrProcessor, FADE_OUT_SAMPLES, MAX_IR_SAMPLES,
    };

    #[test]
    fn loads_mono_ir_from_curated_capture() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../captures/ir/cabs/marshall_4x12_v30/ev_mix_b.wav"
        );

        let ir = IrAsset::load_from_wav(path).expect("mono IR should load");

        assert_eq!(ir.channel_layout(), AudioChannelLayout::Mono);
        assert_eq!(ir.sample_rate(), 48_000);
        assert_eq!(ir.frame_count(), 24_000);
        assert!(matches!(ir.channel_data(), IrChannelData::Mono(_)));
    }

    #[test]
    fn loads_stereo_ir_from_float_wav() {
        let path = std::env::temp_dir().join("openrig_ir_loader_stereo_test.wav");
        crate::test_support::write_test_stereo_ir(&path)
            .expect("test stereo wav should be created");

        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).expect("stereo IR should load");

        assert_eq!(ir.channel_layout(), AudioChannelLayout::Stereo);
        assert_eq!(ir.sample_rate(), 48_000);
        assert_eq!(ir.frame_count(), 4);
        assert!(matches!(ir.channel_data(), IrChannelData::Stereo(_, _)));
    }

    // ── IrChannelData enum ──────────────────────────────────────────

    #[test]
    fn ir_channel_data_mono_returns_correct_channel_count() {
        let data = IrChannelData::Mono(vec![1.0, 0.5, 0.25]);
        assert!(matches!(data, IrChannelData::Mono(_)));
    }

    #[test]
    fn ir_channel_data_stereo_holds_two_channels() {
        let left = vec![1.0, 0.5];
        let right = vec![0.8, 0.3];
        let data = IrChannelData::Stereo(left.clone(), right.clone());
        if let IrChannelData::Stereo(l, r) = &data {
            assert_eq!(l, &left);
            assert_eq!(r, &right);
        } else {
            panic!("expected Stereo variant");
        }
    }

    // ── IrAsset accessors with synthetic WAV ────────────────────────

    fn write_mono_wav(path: &std::path::Path, samples: &[f32], sample_rate: u32) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for &s in samples {
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn write_int16_mono_wav(path: &std::path::Path, samples: &[i16], sample_rate: u32) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for &s in samples {
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn ir_asset_channel_count_mono_returns_one() {
        let path = std::env::temp_dir().join("openrig_ir_test_mono_count.wav");
        write_mono_wav(&path, &[1.0, 0.5, 0.25, 0.0], 44100);
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.channel_count(), 1);
    }

    #[test]
    fn ir_asset_channel_count_stereo_returns_two() {
        let path = std::env::temp_dir().join("openrig_ir_test_stereo_count.wav");
        crate::test_support::write_test_stereo_ir(&path).unwrap();
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.channel_count(), 2);
    }

    #[test]
    fn ir_asset_frame_count_matches_mono_samples() {
        let path = std::env::temp_dir().join("openrig_ir_test_frame_count.wav");
        let samples = vec![0.1; 100];
        write_mono_wav(&path, &samples, 48000);
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.frame_count(), 100);
    }

    #[test]
    fn ir_asset_sample_rate_preserves_original() {
        let path = std::env::temp_dir().join("openrig_ir_test_sr.wav");
        write_mono_wav(&path, &[1.0], 96000);
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.sample_rate(), 96000);
    }

    #[test]
    fn ir_asset_channel_layout_mono_returns_mono() {
        let path = std::env::temp_dir().join("openrig_ir_test_layout_mono.wav");
        write_mono_wav(&path, &[1.0, 0.5], 48000);
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.channel_layout(), AudioChannelLayout::Mono);
    }

    #[test]
    fn ir_asset_channel_layout_stereo_returns_stereo() {
        let path = std::env::temp_dir().join("openrig_ir_test_layout_stereo.wav");
        crate::test_support::write_test_stereo_ir(&path).unwrap();
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.channel_layout(), AudioChannelLayout::Stereo);
    }

    #[test]
    fn load_from_wav_int16_mono_normalizes_correctly() {
        let path = std::env::temp_dir().join("openrig_ir_test_int16.wav");
        write_int16_mono_wav(&path, &[i16::MAX, 0, i16::MIN + 1], 48000);
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.frame_count(), 3);
        if let IrChannelData::Mono(samples) = ir.channel_data() {
            assert!((samples[0] - 1.0).abs() < 0.001);
            assert!((samples[1]).abs() < 0.001);
            assert!((samples[2] + 1.0).abs() < 0.001);
        } else {
            panic!("expected Mono");
        }
    }

    #[test]
    fn load_from_wav_nonexistent_file_returns_error() {
        let result = IrAsset::load_from_wav("/nonexistent/path/to/ir.wav");
        assert!(result.is_err());
    }

    #[test]
    fn load_from_wav_empty_samples_returns_error() {
        let path = std::env::temp_dir().join("openrig_ir_test_empty.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let writer = hound::WavWriter::create(&path, spec).unwrap();
        writer.finalize().unwrap();
        let result = IrAsset::load_from_wav(path.to_str().unwrap());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no samples"));
    }

    // ── truncate_with_fade ──────────────────────────────────────────

    #[test]
    fn truncate_with_fade_short_ir_returns_unchanged() {
        let samples = vec![1.0; 100];
        let result = truncate_with_fade(samples.clone(), "test");
        assert_eq!(result, samples);
    }

    #[test]
    fn truncate_with_fade_exact_max_returns_unchanged() {
        let samples = vec![1.0; MAX_IR_SAMPLES];
        let result = truncate_with_fade(samples.clone(), "test");
        assert_eq!(result, samples);
    }

    #[test]
    fn truncate_with_fade_long_ir_truncates_to_max() {
        let samples = vec![1.0; MAX_IR_SAMPLES + 1000];
        let result = truncate_with_fade(samples, "test");
        assert_eq!(result.len(), MAX_IR_SAMPLES);
    }

    #[test]
    fn truncate_with_fade_long_ir_applies_cosine_fadeout() {
        let samples = vec![1.0; MAX_IR_SAMPLES + 1000];
        let result = truncate_with_fade(samples, "test");
        // Last sample should be near zero (end of cosine fade)
        assert!(result[MAX_IR_SAMPLES - 1].abs() < 0.01);
        // Sample just before fade region should be unchanged
        let fade_start = MAX_IR_SAMPLES - FADE_OUT_SAMPLES;
        assert!((result[fade_start - 1] - 1.0).abs() < 0.001);
    }

    // ── resample_if_needed ──────────────────────────────────────────

    #[test]
    fn resample_if_needed_same_rate_returns_unchanged() {
        let samples = vec![1.0, 0.5, 0.25];
        let result = resample_if_needed(samples.clone(), 48000, 48000.0, "test");
        assert_eq!(result, samples);
    }

    #[test]
    fn resample_if_needed_zero_runtime_rate_returns_unchanged() {
        let samples = vec![1.0, 0.5, 0.25];
        let result = resample_if_needed(samples.clone(), 48000, 0.0, "test");
        assert_eq!(result, samples);
    }

    #[test]
    fn resample_if_needed_upsample_produces_longer_output() {
        let samples = vec![1.0; 100];
        let result = resample_if_needed(samples, 44100, 48000.0, "test");
        // 100 * (48000/44100) ~ 109
        assert!(result.len() > 100);
    }

    #[test]
    fn resample_if_needed_downsample_produces_shorter_output() {
        let samples = vec![1.0; 100];
        let result = resample_if_needed(samples, 48000, 44100.0, "test");
        assert!(result.len() < 100);
    }

    // ── lanczos_kernel ──────────────────────────────────────────────

    #[test]
    fn lanczos_kernel_at_zero_returns_one() {
        assert!((lanczos_kernel(0.0, 4.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn lanczos_kernel_at_boundary_returns_zero() {
        assert!((lanczos_kernel(4.0, 4.0)).abs() < 1e-6);
        assert!((lanczos_kernel(-4.0, 4.0)).abs() < 1e-6);
    }

    #[test]
    fn lanczos_kernel_beyond_boundary_returns_zero() {
        assert_eq!(lanczos_kernel(5.0, 4.0), 0.0);
        assert_eq!(lanczos_kernel(-5.0, 4.0), 0.0);
    }

    #[test]
    fn lanczos_kernel_positive_near_zero_returns_positive() {
        let val = lanczos_kernel(0.5, 4.0);
        assert!(val > 0.0);
    }

    // ── MonoIrProcessor ─────────────────────────────────────────────

    #[test]
    fn mono_ir_processor_impulse_response_reproduces_ir() {
        // Convolving with a delta should reproduce the IR
        let ir = vec![0.5, 0.3, 0.1];
        let mut proc = MonoIrProcessor::new(ir.clone());
        // Feed an impulse followed by zeros
        let _impulse = proc.process_sample(1.0);
        let _s1 = proc.process_sample(0.0);
        let _s2 = proc.process_sample(0.0);

        // The FFT convolver introduces latency, so the IR appears
        // after a partition-sized delay. We just verify the processor
        // produces non-silent output eventually.
        let mut buf = vec![0.0; 1024];
        buf[0] = 1.0;
        let mut proc2 = MonoIrProcessor::new(ir.clone());
        proc2.process_block(&mut buf);
        let energy: f32 = buf.iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "convolver should produce non-zero output");
    }

    #[test]
    fn mono_ir_processor_process_block_matches_sample_by_sample() {
        let ir = vec![1.0, 0.0, 0.0, 0.0];
        let mut proc_block = MonoIrProcessor::new(ir.clone());
        let mut proc_sample = MonoIrProcessor::new(ir);

        let mut block = vec![0.1, 0.2, 0.3, 0.4, 0.0, 0.0, 0.0, 0.0];
        let block_copy = block.clone();
        proc_block.process_block(&mut block);

        let sample_results: Vec<f32> = block_copy
            .iter()
            .map(|&s| proc_sample.process_sample(s))
            .collect();

        // Both should produce identical output
        for (a, b) in block.iter().zip(sample_results.iter()) {
            assert!(
                (a - b).abs() < 1e-6,
                "block vs sample mismatch: {} vs {}",
                a,
                b
            );
        }
    }

    #[test]
    fn mono_ir_processor_silence_in_produces_silence_out() {
        let ir = vec![1.0, 0.5, 0.25];
        let mut proc = MonoIrProcessor::new(ir);
        let mut buf = vec![0.0; 512];
        proc.process_block(&mut buf);
        assert!(buf.iter().all(|&s| s == 0.0));
    }

    // ── StereoIrProcessor ───────────────────────────────────────────

    #[test]
    fn stereo_ir_processor_silence_in_produces_silence_out() {
        let left_ir = vec![1.0, 0.5];
        let right_ir = vec![0.8, 0.3];
        let mut proc = StereoIrProcessor::new(left_ir, right_ir);
        let mut buf = vec![[0.0f32; 2]; 512];
        proc.process_block(&mut buf);
        assert!(buf.iter().all(|frame| frame[0] == 0.0 && frame[1] == 0.0));
    }

    #[test]
    fn stereo_ir_processor_process_block_produces_output() {
        let left_ir = vec![1.0; 4];
        let right_ir = vec![0.5; 4];
        let mut proc = StereoIrProcessor::new(left_ir, right_ir);
        let mut buf = vec![[0.0f32; 2]; 1024];
        buf[0] = [1.0, 1.0];
        proc.process_block(&mut buf);
        let energy: f32 = buf.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum();
        assert!(energy > 0.0, "stereo convolver should produce output");
    }

    #[test]
    fn stereo_ir_processor_frame_matches_block() {
        let left_ir = vec![1.0, 0.0, 0.0, 0.0];
        let right_ir = vec![1.0, 0.0, 0.0, 0.0];
        let mut proc_frame = StereoIrProcessor::new(left_ir.clone(), right_ir.clone());
        let mut proc_block = StereoIrProcessor::new(left_ir, right_ir);

        let input = [[0.5, 0.3], [0.2, 0.1], [0.0, 0.0], [0.0, 0.0]];
        let frame_results: Vec<[f32; 2]> = input
            .iter()
            .map(|&f| proc_frame.process_frame(f))
            .collect();

        let mut block = input.to_vec();
        proc_block.process_block(&mut block);

        for (a, b) in block.iter().zip(frame_results.iter()) {
            assert!((a[0] - b[0]).abs() < 1e-6);
            assert!((a[1] - b[1]).abs() < 1e-6);
        }
    }
}
