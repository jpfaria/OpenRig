//! A stream that feeds several outputs shows one L/R pair per output, and
//! every pair reads the whole signal of the stream's single tap.

use super::*;
use application::audio_taps::{AudioTap, AudioTaps, TapPoint};
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use feature_dsp::spectrum_fft::HOP_SIZE;
use project::chain::Chain;
use std::collections::VecDeque;
use std::sync::Mutex;

struct QueueTap {
    channels: Mutex<Vec<VecDeque<f32>>>,
}

impl AudioTap for QueueTap {
    fn channels(&self) -> usize {
        2
    }
    fn poll_peak_dbfs(&self) -> f32 {
        0.0
    }
    fn drain_channel(&self, channel: usize, max: usize, out: &mut Vec<f32>) -> usize {
        let mut channels = self.channels.lock().unwrap();
        let queue = &mut channels[channel];
        let n = max.min(queue.len());
        out.extend(queue.drain(..n));
        n
    }
}

struct OneStream(Arc<QueueTap>);

impl AudioTaps for OneStream {
    fn is_hosted(&self) -> bool {
        true
    }
    fn live_sample_rate(&self) -> u32 {
        48_000
    }
    fn stream_count(&self, _chain: &ChainId) -> usize {
        1
    }
    fn subscribe(&self, _point: &TapPoint, _capacity: usize) -> Option<Arc<dyn AudioTap>> {
        Some(self.0.clone())
    }
}

fn ep(name: &str, channels: Vec<usize>, mode: ChannelMode) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev:1".into()),
        mode,
        channels,
    }
}

fn guitar(outputs: Vec<IoEndpoint>) -> Vec<IoBinding> {
    vec![IoBinding {
        id: "g1".into(),
        name: "G1".into(),
        inputs: vec![ep("in", vec![0], ChannelMode::Mono)],
        outputs,
    }]
}

fn project() -> Project {
    Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("chain:0".into()),
            description: Some("Digital".into()),
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["g1".into()],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
            disabled_endpoints: Default::default(),
            mix: Default::default(),
        }],
        midi: None,
    }
}

fn sine() -> VecDeque<f32> {
    (0..HOP_SIZE)
        .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 48_000.0).sin() * 0.5)
        .collect()
}

#[test]
fn every_output_of_a_stream_gets_its_own_pair_of_rows() {
    let registry = guitar(vec![
        ep("main", vec![0, 1], ChannelMode::Stereo),
        ep("frfr", vec![24, 25], ChannelMode::Stereo),
    ]);
    let tap = Arc::new(QueueTap {
        channels: Mutex::new(vec![sine(), sine()]),
    });

    let mut session = SpectrumSession::build(&project(), &OneStream(tap), &registry);
    let labels: Vec<String> = session
        .rows_model
        .iter()
        .map(|row| row.label.to_string())
        .collect();
    assert_eq!(
        labels,
        vec![
            "DIGITAL  ·  IN 1  →  OUT 1,2  ·  L",
            "DIGITAL  ·  IN 1  →  OUT 1,2  ·  R",
            "DIGITAL  ·  IN 1  →  OUT 25,26  ·  L",
            "DIGITAL  ·  IN 1  →  OUT 25,26  ·  R",
        ]
    );

    session.tick();
    let levels = |idx: usize| -> Vec<f32> {
        session
            .rows_model
            .row_data(idx)
            .unwrap()
            .levels
            .iter()
            .collect()
    };
    assert!(
        levels(0).iter().any(|&l| l > 0.0),
        "the L row reads the signal"
    );
    assert_eq!(levels(0), levels(2), "both outputs show the whole stream");
    assert_eq!(levels(1), levels(3));
}

#[test]
fn fingerprint_changes_when_an_output_is_added() {
    let one = guitar(vec![ep("main", vec![0, 1], ChannelMode::Stereo)]);
    let two = guitar(vec![
        ep("main", vec![0, 1], ChannelMode::Stereo),
        ep("frfr", vec![24, 25], ChannelMode::Stereo),
    ]);
    assert_ne!(
        project_stream_fingerprint(&project(), &one),
        project_stream_fingerprint(&project(), &two)
    );
}
