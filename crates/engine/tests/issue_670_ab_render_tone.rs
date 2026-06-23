//! Issue #670 — A/B tone render. The user reports the TONE went bad ("som ta
//! uma bosta") after today's merges (#675 ADAA peak-safety, #689 oversampling).
//! This renders the real Beat It chain with the real Green Day DI to a WAV so
//! the SAME render can be produced from different commits and compared
//! numerically. Output path: $ISSUE670_RENDER_OUT or /tmp/issue670_render.wav.
#![cfg(not(debug_assertions))]

use std::path::PathBuf;
use std::sync::Once;

use domain::ids::{BlockId, ChainId, DeviceId};
use engine::offline::render_chain;
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};

const SR: f32 = 48_000.0;

fn init() {
    static I: Once = Once::new();
    I.call_once(|| {
        nam::register_builder();
        ir::register_builder();
        lv2::register_builder();
        block_dyn::register_natives();
        block_filter::register_natives();
        block_reverb::register_natives();
        block_gain::register_natives();
        block_amp::register_natives();
        block_preamp::register_natives();
        block_cab::register_natives();
        block_delay::register_natives();
        block_mod::register_natives();
        block_pitch::register_natives();
        plugin_loader::registry::init(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins"),
        );
    });
}

#[test]
fn render_beat_it_green_day_to_wav() {
    init();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let preset = manifest.join("tests/fixtures/presets/beat_it_michael_jackson_rhythm.yaml");
    let mut blocks = vec![AudioBlock {
        id: BlockId("in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }];
    blocks.extend(
        infra_yaml::load_chain_preset_file(&preset)
            .expect("preset")
            .blocks,
    );
    blocks.push(AudioBlock {
        id: BlockId("out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    });
    let chain = Chain {
        id: ChainId("ab".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 139.0,
        io_binding_ids: vec![],
        blocks,
    };

    // 12 s of the real Green Day DI.
    let di_path = manifest.join("../../assets/di-loops/phil-STRATO-green_day.wav");
    let mut reader = hound::WavReader::open(&di_path).expect("DI");
    let spec = reader.spec();
    let mono: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.unwrap())
            .step_by(spec.channels as usize)
            .take((SR as usize) * 12)
            .collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.unwrap() as f32 / max)
                .step_by(spec.channels as usize)
                .take((SR as usize) * 12)
                .collect()
        }
    };
    let input: Vec<[f32; 2]> = mono.iter().map(|&s| [s, s]).collect();

    let out = render_chain(&chain, SR, &input, 64, 0)
        .expect("render")
        .samples;

    let path =
        std::env::var("ISSUE670_RENDER_OUT").unwrap_or_else(|_| "/tmp/issue670_render.wav".into());
    let wspec = hound::WavSpec {
        channels: 2,
        sample_rate: SR as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(&path, wspec).expect("wav out");
    for s in &out {
        w.write_sample(s[0]).unwrap();
        w.write_sample(s[1]).unwrap();
    }
    w.finalize().unwrap();
    eprintln!("[#670 AB] rendered {} frames to {path}", out.len());
}
