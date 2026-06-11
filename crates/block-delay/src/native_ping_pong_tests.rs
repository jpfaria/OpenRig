use super::*;
use crate::dsp_probe;
use block_core::StereoProcessor;

const SR: f32 = 48_000.0;

fn ping_pong(time_ms: f32, feedback: f32, mix: f32) -> PingPongDelay {
    PingPongDelay::new(
        PingPongParams {
            time_ms,
            feedback,
            mix,
        },
        SR,
    )
}

/// Feed a left-only impulse, return (left_channel, right_channel) renders.
fn render_left_impulse(delay: &mut PingPongDelay, len: usize) -> (Vec<f32>, Vec<f32>) {
    let mut left = Vec::with_capacity(len);
    let mut right = Vec::with_capacity(len);
    for i in 0..len {
        let frame = if i == 0 { [1.0, 0.0] } else { [0.0, 0.0] };
        let [l, r] = delay.process_frame(frame);
        left.push(l);
        right.push(r);
    }
    (left, right)
}

// --- Proposal: the echo bounces L↔R (true stereo, not two independent mono) ---

#[test]
fn ping_pong_first_echo_lands_on_the_opposite_channel() {
    let time_ms = 100.0;
    let delay_samples = (time_ms * 0.001 * SR) as usize; // 4_800
    let mut delay = ping_pong(time_ms, 0.6, 1.0); // fully wet

    let (left, right) = render_left_impulse(&mut delay, 20_000);

    // The left input should appear FIRST on the right channel.
    let win = 600usize;
    let r_energy: f32 = right[delay_samples - win..delay_samples + win]
        .iter()
        .map(|s| s * s)
        .sum();
    let l_energy: f32 = left[delay_samples - win..delay_samples + win]
        .iter()
        .map(|s| s * s)
        .sum();
    assert!(
        r_energy > l_energy * 8.0,
        "first echo of a left input must bounce to the right: R={r_energy:.4} L={l_energy:.4}"
    );
}

#[test]
fn ping_pong_second_echo_returns_to_the_origin_channel() {
    let time_ms = 100.0;
    let delay_samples = (time_ms * 0.001 * SR) as usize;
    let mut delay = ping_pong(time_ms, 0.6, 1.0);

    let (left, right) = render_left_impulse(&mut delay, 20_000);

    // First echo on right (~delay), second back on left (~2·delay).
    let r_peak = dsp_probe::peaks(&right, 0.02, delay_samples / 2)[0].0;
    let l_peak = dsp_probe::peaks(&left, 0.02, delay_samples / 2)[0].0;
    assert!(
        (r_peak as i64 - delay_samples as i64).abs() <= 40,
        "right echo should be at ~{delay_samples}, got {r_peak}"
    );
    assert!(
        (l_peak as i64 - 2 * delay_samples as i64).abs() <= 40,
        "left echo should return at ~{}, got {l_peak}",
        2 * delay_samples
    );
}

// --- Denormal / NaN guards ---

#[test]
fn ping_pong_outputs_finite_values() {
    let mut delay = PingPongDelay::new(PingPongParams::default(), 48_000.0);
    for _ in 0..10_000 {
        let [l, r] = delay.process_frame([0.2, 0.1]);
        assert!(l.is_finite() && r.is_finite());
    }
}

#[test]
fn ping_pong_silence_output_is_finite() {
    let mut delay = PingPongDelay::new(PingPongParams::default(), 44100.0);
    for i in 0..1024 {
        let [l, r] = delay.process_frame([0.0, 0.0]);
        assert!(l.is_finite() && r.is_finite(), "non-finite at {i}");
    }
}
