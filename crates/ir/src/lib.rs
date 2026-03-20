use anyhow::{bail, Context, Result};
use realfft::{num_complex::Complex32, ComplexToReal, RealFftPlanner, RealToComplex};
use stage_core::{AudioChannelLayout, MonoProcessor, StereoProcessor};
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

pub fn build_mono_ir_processor_from_wav(
    path: &str,
    runtime_sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    let ir = IrAsset::load_from_wav(path)?;
    if ir.channel_count() != 1 {
        bail!("IR '{}' is not mono", path);
    }
    validate_sample_rate(&ir, runtime_sample_rate, path)?;
    let IrChannelData::Mono(samples) = ir.channel_data else {
        unreachable!()
    };
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
    validate_sample_rate(&ir, runtime_sample_rate, path)?;
    let IrChannelData::Stereo(left, right) = ir.channel_data else {
        unreachable!()
    };
    Ok(Box::new(StereoIrProcessor::new(left, right)))
}

fn validate_sample_rate(ir: &IrAsset, runtime_sample_rate: f32, path: &str) -> Result<()> {
    let runtime_sample_rate = runtime_sample_rate.round() as u32;
    if runtime_sample_rate == 0 {
        bail!("runtime sample rate must be greater than zero");
    }
    if ir.sample_rate != runtime_sample_rate {
        bail!(
            "IR '{}' uses sample_rate {} but runtime is {}",
            path,
            ir.sample_rate,
            runtime_sample_rate
        );
    }
    Ok(())
}

struct FftBlockConvolver {
    ir: Vec<f32>,
    state: Option<FftConvolverState>,
}

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
        self.ensure_state(buffer.len());
        let state = self.state.as_mut().expect("fft state initialized");
        state.input.fill(0.0);
        state.input[..buffer.len()].copy_from_slice(buffer);
        state
            .forward
            .process(&mut state.input, &mut state.spectrum)
            .expect("forward FFT should succeed");

        for (bin, ir_bin) in state.spectrum.iter_mut().zip(state.ir_spectrum.iter()) {
            *bin *= *ir_bin;
        }

        state
            .inverse
            .process(&mut state.spectrum, &mut state.output)
            .expect("inverse FFT should succeed");

        let scale = state.fft_len as f32;
        for sample in &mut state.output {
            *sample /= scale;
        }

        for (index, sample) in buffer.iter_mut().enumerate() {
            *sample = state.output[index] + state.overlap.get(index).copied().unwrap_or(0.0);
        }

        if !state.overlap.is_empty() {
            let block_len = buffer.len();
            for (index, sample) in state.overlap.iter_mut().enumerate() {
                *sample = state.output[block_len + index];
            }
        }
    }

    fn ensure_state(&mut self, block_len: usize) {
        if self
            .state
            .as_ref()
            .is_some_and(|state| state.block_len == block_len)
        {
            return;
        }

        let tail_len = self.ir.len().saturating_sub(1);
        let fft_len = (block_len + tail_len).next_power_of_two().max(2);
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(fft_len);
        let inverse = planner.plan_fft_inverse(fft_len);

        let mut ir_input = forward.make_input_vec();
        ir_input[..self.ir.len()].copy_from_slice(&self.ir);
        let mut ir_spectrum = forward.make_output_vec();
        forward
            .process(&mut ir_input, &mut ir_spectrum)
            .expect("IR FFT should succeed");

        self.state = Some(FftConvolverState {
            block_len,
            fft_len,
            forward,
            inverse,
            input: vec![0.0; fft_len],
            spectrum: vec![Complex32::default(); fft_len / 2 + 1],
            output: vec![0.0; fft_len],
            ir_spectrum,
            overlap: vec![0.0; tail_len],
        });
    }
}

struct FftConvolverState {
    block_len: usize,
    fft_len: usize,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    input: Vec<f32>,
    spectrum: Vec<Complex32>,
    output: Vec<f32>,
    ir_spectrum: Vec<Complex32>,
    overlap: Vec<f32>,
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
    use stage_core::AudioChannelLayout;

    use crate::{IrAsset, IrChannelData};

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
}
