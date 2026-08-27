//! Responsibility: convolves a signal with an impulse response in partitioned FFT blocks.
//!
//! Split out of `lib.rs` (#873). The planner, the per-partition FFTs and every
//! buffer are allocated at construction, because `process` runs on the audio
//! thread.

use anyhow::{bail, Result};
use realfft::{num_complex::Complex32, ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// Uniformly partitioned FFT convolver with internal buffering.
///
/// Splits the IR into fixed-size segments, pre-computes their FFTs, and
/// convolves with small FFTs. Internal buffering decouples audio block size
/// from partition size — works efficiently with any buffer size.
pub(crate) struct FftBlockConvolver {
    ir: Vec<f32>,
    state: Option<PartitionedState>,
}

/// Convolution partition size, in samples.
///
/// This is the granularity at which the convolver runs its inline FFT +
/// frequency-delay-line multiply-accumulate: that work happens once per
/// `PARTITION_SIZE` input samples. It MUST stay at or below the smallest
/// real-time device buffer we support (64) so the work is spread evenly
/// across callbacks. When `PARTITION_SIZE` is larger than the device
/// buffer the whole convolution burst lands inside a single callback —
/// a periodic spike that overruns small buffers and crackles/clips
/// (issue #617; the symptom that 128-frame buffers masked because their
/// larger per-callback budget could absorb the burst). Keeping it at 64
/// makes each 64-frame callback do exactly one partition's worth of work
/// (uniform cost, no spike) and also bounds the convolver's added latency
/// to one partition (~1.3 ms at 48 kHz) instead of ~10.7 ms.
///
/// Trade-off (per the audio invariant hierarchy: stability > CPU): more
/// partitions for a long IR (up to 128 for MAX_IR_SAMPLES=8192) raise the
/// *average* per-callback cost, but it stays tiny in absolute terms and,
/// crucially, uniform — no callback misses its deadline.
///
/// Still exposed because the engine's output elastic buffer floors its
/// jitter cushion at one partition for IR chains (issue #592). With the
/// per-partition spike removed at the source, that cushion is now only a
/// warmup belt-and-suspenders rather than the primary defence.
pub const PARTITION_SIZE: usize = 64;

impl FftBlockConvolver {
    pub(crate) fn new(ir: Vec<f32>) -> Result<Self> {
        if ir.is_empty() {
            bail!("IR cannot be empty");
        }
        let mut convolver = Self { ir, state: None };
        // Issue #670: build the partitioned state (FFT planning + buffer
        // allocations) EAGERLY, on the thread that constructs the block (the
        // build/rebuild thread) — not lazily on the audio worker's first
        // process call, where the one-time multi-hundred-us spike lands right
        // after a cab/model swap and can starve the output.
        convolver.ensure_state();
        Ok(convolver)
    }

    pub(crate) fn process_block_in_place(&mut self, buffer: &mut [f32]) {
        if buffer.is_empty() {
            return;
        }
        self.ensure_state();
        let state = self.state.as_mut().expect("partitioned state initialized");

        // Feed samples into internal input buffer, process partition-sized
        // chunks, and drain output buffer back into the caller's buffer.
        let out_len = state.output_buf.len();
        for sample in buffer.iter_mut() {
            state.input_buf[state.input_pos] = *sample;
            *sample = state.output_buf[state.output_pos % out_len];
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
        state
            .forward
            .process(&mut state.fft_input, &mut state.fft_scratch)
            .expect("forward FFT");

        // Frequency delay line: shift down one partition (newest at the front)
        // so the multiply-accumulate below walks BOTH `fdl` and `ir_partitions`
        // strictly SEQUENTIALLY. Issue #670: the old code stored the FDL in a
        // ring and indexed it with a modular, non-sequential offset; that
        // working set (~130 KB) is fine while hot, but once the OS
        // context-switches the audio thread to the UI (Slint render + spectrum
        // FFT) and back, the cold-cache reload becomes a per-element miss storm
        // and the convolution balloons ~67x (the live ~270 us/buffer IR spike
        // the user heard as a "beehive" with ANY IR). A linear shift + linear
        // MAC reload as a prefetchable stream instead. The shift is a single
        // contiguous memmove; the convolution result is unchanged.
        let shift_span = (state.num_partitions - 1) * spectrum_len;
        state.fdl.copy_within(0..shift_span, spectrum_len);
        state.fdl[..spectrum_len].copy_from_slice(&state.fft_scratch);

        // Multiply-accumulate across all IR partitions — fdl[p] is the input
        // spectrum from p partitions ago, ir_partitions[p] the p-th IR
        // partition; both indexed by the same sequential `p`.
        state
            .accum
            .iter_mut()
            .for_each(|c| *c = Complex32::default());
        for p in 0..state.num_partitions {
            let off = p * spectrum_len;
            for i in 0..spectrum_len {
                state.accum[i] += state.fdl[off + i] * state.ir_partitions[off + i];
            }
        }

        // The DC (bin 0) and Nyquist (last bin) components of a real signal's
        // spectrum are purely real. Multiplying and accumulating per-partition
        // spectra leaves tiny imaginary residues there from f32 round-off,
        // which `realfft`'s strict inverse rejects ("imaginary parts of first
        // and last values were non-zero"). They are mathematically zero, so
        // clamp them — this is round-off cleanup, not a signal change.
        let nyquist = spectrum_len - 1;
        state.accum[0].im = 0.0;
        state.accum[nyquist].im = 0.0;

        // Inverse FFT
        state
            .inverse
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

        let num_partitions = self.ir.len().div_ceil(ps);
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
            forward
                .process(&mut buf, &mut out)
                .expect("IR partition FFT");
            let offset = p * spectrum_len;
            ir_partitions[offset..offset + spectrum_len].copy_from_slice(&out);
        }

        // Output ring buffer must be large enough to hold the full
        // convolution result overlap: num_partitions * partition_size + fft_len
        let output_buf_len = (num_partitions + 1) * ps + fft_len;

        log::debug!(
            "IR convolver: {} samples, {} partitions of {}, fft_len={}, output_buf={}",
            self.ir.len(),
            num_partitions,
            ps,
            fft_len,
            output_buf_len
        );

        self.state = Some(PartitionedState {
            partition_size: ps,
            fft_len,
            num_partitions,
            forward,
            inverse,
            ir_partitions,
            fdl: vec![Complex32::default(); num_partitions * spectrum_len],
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
    input_buf: Vec<f32>,
    input_pos: usize,
    output_buf: Vec<f32>,
    output_pos: usize,
    fft_input: Vec<f32>,
    fft_output: Vec<f32>,
    fft_scratch: Vec<Complex32>,
    accum: Vec<Complex32>,
}
