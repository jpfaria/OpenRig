//! Issue #670 — render REAL sound through the IR offline and detect the
//! "beehive" (buzz) the user hears, WITHOUT asking the user to listen.
//!
//! An IR/CAB is a LINEAR convolution: a pure sine in must come out a pure
//! sine (only the cab's frequency shaping, no NEW frequencies). So any
//! energy at a frequency unrelated to the input tone is an artifact. A
//! partitioned-convolution overlap-add bug shows up as a buzz at the
//! partition rate (sample_rate / PARTITION_SIZE = 48000/64 = 750 Hz) and its
//! harmonics — exactly a "caixa de abelha". This renders the real preset's IR
//! block with a pure tone and asserts the output stays clean.
//!
//! Release-gated only because the analysis FFT is slow in debug; the bug
//! itself is deterministic.

#![cfg_attr(debug_assertions, allow(unused))]

use std::path::PathBuf;
use std::sync::Once;

use domain::ids::{BlockId, ChainId, DeviceId};
use engine::offline::render_chain;
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

const SR: f32 = 48_000.0;

fn init() {
    static I: Once = Once::new();
    I.call_once(|| {
        ir::register_builder();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins");
        plugin_loader::registry::init(&root);
    });
}

fn ir_block_from_preset() -> AudioBlock {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/presets/beat_it_michael_jackson_rhythm.yaml");
    let blocks = infra_yaml::load_chain_preset_file(&path)
        .expect("load preset")
        .blocks;
    blocks
        .into_iter()
        .find(|b| matches!(&b.kind, AudioBlockKind::Core(c) if c.model.starts_with("ir_")))
        .expect("preset must have an IR block")
}

fn io_wrap(block: AudioBlock) -> Chain {
    Chain {
        id: ChainId("ir-artifact".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            AudioBlock {
                id: BlockId("in".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("d".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            block,
            AudioBlock {
                id: BlockId("out".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("d".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    }
}

/// Peak magnitude in a small band around `f` Hz.
fn band(mag: &[f32], bin_hz: f32, f: f32) -> f32 {
    let b = (f / bin_hz).round() as usize;
    let lo = b.saturating_sub(2);
    let hi = (b + 3).min(mag.len());
    mag.get(lo..hi).map(|w| w.iter().cloned().fold(0.0, f32::max)).unwrap_or(0.0)
}

#[test]
#[cfg_attr(debug_assertions, ignore = "analysis FFT is slow in debug")]
fn ir_output_is_clean_no_partition_rate_buzz() {
    init();
    let chain = io_wrap(ir_block_from_preset());
    let outcome = render_chain(&chain, SR, &[[0.0, 0.0]; 8], 64, 0).expect("render");
    assert!(outcome.faulted_blocks.is_empty(), "IR faulted: {:?}", outcome.faulted_blocks);

    let n = 16_384usize;
    let freq = 220.0f32;
    let input: Vec<[f32; 2]> = (0..n)
        .map(|i| {
            let s = 0.3 * (2.0 * std::f32::consts::PI * freq * i as f32 / SR).sin();
            [s, s]
        })
        .collect();

    for &block in &[64usize, 32, 96, 128] {
        let out = render_chain(&chain, SR, &input, block, 0).expect("render").samples;
        // steady-state window (skip attack + the IR's latency)
        let sig: Vec<f32> = out[4096..12_288].iter().map(|s| s[0]).collect();
        let len = sig.len();
        let mut buf: Vec<Complex<f32>> = sig.iter().map(|&s| Complex::new(s, 0.0)).collect();
        FftPlanner::new().plan_fft_forward(len).process(&mut buf);
        let mag: Vec<f32> = buf[..len / 2].iter().map(|c| c.norm()).collect();
        let bin_hz = SR / len as f32;

        let fund = band(&mag, bin_hz, freq);
        let part_rate = SR / 64.0; // 750 Hz
        let buzz = band(&mag, bin_hz, part_rate)
            .max(band(&mag, bin_hz, part_rate * 2.0))
            .max(band(&mag, bin_hz, part_rate * 3.0))
            .max(band(&mag, bin_hz, part_rate - freq))
            .max(band(&mag, bin_hz, part_rate + freq));
        eprintln!(
            "[#670 IR] block={block:>3}: fundamental(220Hz)={:.3} partition-buzz(~750Hz)={:.3} ratio={:.4}",
            fund,
            buzz,
            buzz / fund.max(1e-9),
        );
        assert!(
            buzz < fund * 0.02,
            "BUG #670: at block_size={block} the IR output has a partition-rate \
             (~750 Hz) buzz {buzz:.3} vs the 220 Hz fundamental {fund:.3} \
             ({:.1}%) — a linear convolution must not create that frequency. \
             This is the 'caixa de abelha'.",
            buzz / fund.max(1e-9) * 100.0,
        );
    }
}
