//! #948: with two guitars on the chain, the Tone Doctor hears both, summed — the
//! owner's rule. Before, only the first stream was captured.

use std::sync::Arc;

use application::audio_taps::{AudioTap, AudioTaps, TapPoint};
use domain::ids::ChainId;

use super::live_capture;

const SR: u32 = 100;

/// A tap that always has `level` to give.
struct ConstantTap(f32);

impl AudioTap for ConstantTap {
    fn channels(&self) -> usize {
        1
    }
    fn poll_peak_dbfs(&self) -> f32 {
        0.0
    }
    fn drain_channel(&self, _channel: usize, max: usize, out: &mut Vec<f32>) -> usize {
        out.extend(std::iter::repeat_n(self.0, max));
        max
    }
}

/// Guitar 1 plays 0.1, guitar 2 plays 0.2.
struct TwoGuitars;

impl AudioTaps for TwoGuitars {
    fn is_hosted(&self) -> bool {
        true
    }
    fn live_sample_rate(&self) -> u32 {
        SR
    }
    fn stream_count(&self, _chain: &ChainId) -> usize {
        2
    }
    fn subscribe(&self, point: &TapPoint, _capacity: usize) -> Option<Arc<dyn AudioTap>> {
        match point {
            TapPoint::StreamInput { stream: 0, .. } => Some(Arc::new(ConstantTap(0.1))),
            TapPoint::StreamInput { stream: 1, .. } => Some(Arc::new(ConstantTap(0.2))),
            _ => None,
        }
    }
}

#[test]
fn both_guitars_are_captured_and_summed() {
    let capture = live_capture(&TwoGuitars, &ChainId("rig:input-4".into()), 1)
        .expect("a hosted chain with streams has a live input");
    let (frames, sr) = capture().expect("the window fills");

    assert_eq!(sr, SR as f32);
    assert_eq!(frames.len(), SR as usize, "one second of frames");
    for frame in &frames {
        assert!(
            (frame[0] - 0.3).abs() < 1e-6 && (frame[1] - 0.3).abs() < 1e-6,
            "#948: guitar 1 (0.1) + guitar 2 (0.2) must be summed, got {frame:?}"
        );
    }
}
