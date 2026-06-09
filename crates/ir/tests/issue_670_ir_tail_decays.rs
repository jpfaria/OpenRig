//! Issue #670: the partitioned IR convolution must decay to SILENCE when the
//! input goes silent. If the overlap-add / output-ring bookkeeping leaves a
//! residue, the output keeps buzzing on a dead signal — a constant
//! "caixa de abelha" with ANY IR, which is what the user hears.

use block_core::MonoProcessor;
use ir::MonoIrProcessor;

const SR: f32 = 48_000.0;

fn cabinet_ir(len: usize) -> Vec<f32> {
    (0..len)
        .map(|n| {
            let t = n as f32 / SR;
            (-t * 25.0).exp() * (2.0 * std::f32::consts::PI * 1500.0 * t).sin()
        })
        .collect()
}

#[test]
fn ir_output_decays_to_silence_on_silent_input() {
    for &ir_len in &[2048usize, 4096, 8192] {
        for &block in &[64usize, 32, 128, 96, 100] {
            let mut proc = MonoIrProcessor::new(cabinet_ir(ir_len));

            // Drive a loud tone burst to fill the convolver's state.
            let mut peak_driven = 0.0f32;
            for k in 0..(8192 / block + 4) {
                let mut buf: Vec<f32> = (0..block)
                    .map(|i| {
                        let n = (k * block + i) as f32;
                        0.5 * (2.0 * std::f32::consts::PI * 220.0 * n / SR).sin()
                    })
                    .collect();
                proc.process_block(&mut buf);
                for &v in &buf {
                    peak_driven = peak_driven.max(v.abs());
                }
            }

            // Now feed pure silence for well beyond the IR length and check
            // the output dies. Convolving zeros with anything is zero.
            let silence_blocks = (ir_len * 3) / block + 8;
            let mut tail_peak = 0.0f32;
            for k in 0..silence_blocks {
                let mut buf = vec![0.0f32; block];
                proc.process_block(&mut buf);
                // ignore the first IR-length of decay; measure the FAR tail.
                if k > (ir_len * 2) / block {
                    for &v in &buf {
                        tail_peak = tail_peak.max(v.abs());
                    }
                }
            }

            eprintln!(
                "[#670 IR] ir_len={ir_len} block={block}: driven_peak={peak_driven:.4} far_tail_peak={tail_peak:.8}"
            );
            assert!(
                tail_peak < 1e-4,
                "BUG #670: IR (len={ir_len}, block={block}) keeps emitting \
                 {tail_peak:.6} after the input has been silent for 3x the IR \
                 length — the convolution does not decay to silence, it BUZZES \
                 (the caixa de abelha). driven peak was {peak_driven:.3}."
            );
        }
    }
}
