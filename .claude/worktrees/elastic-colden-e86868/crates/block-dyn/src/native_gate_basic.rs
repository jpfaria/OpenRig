use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{db_to_lin, EnvelopeFollower, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "gate_basic";
pub const DISPLAY_NAME: &str = "Noise Gate";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateParams {
    pub threshold: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub hold_ms: f32,
    pub hysteresis_db: f32,
}

impl Default for GateParams {
    fn default() -> Self {
        Self {
            threshold: 35.0,
            attack_ms: 5.0,
            release_ms: 50.0,
            hold_ms: 150.0,
            hysteresis_db: 6.0,
        }
    }
}

fn percent_to_threshold_db(p: f32) -> f32 {
    -96.0 + (p / 100.0) * 96.0
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "dynamics".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Noise Gate".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "threshold",
                "Threshold",
                None,
                Some(GateParams::default().threshold),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "attack_ms",
                "Attack",
                None,
                Some(GateParams::default().attack_ms),
                0.1,
                100.0,
                0.1,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "release_ms",
                "Release",
                None,
                Some(GateParams::default().release_ms),
                1.0,
                500.0,
                0.1,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "hold_ms",
                "Hold",
                None,
                Some(GateParams::default().hold_ms),
                0.0,
                2000.0,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "hysteresis_db",
                "Hysteresis",
                None,
                Some(GateParams::default().hysteresis_db),
                0.0,
                20.0,
                0.5,
                ParameterUnit::Decibels,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<GateParams> {
    Ok(GateParams {
        threshold: percent_to_threshold_db(required_f32(params, "threshold").map_err(Error::msg)?),
        attack_ms: required_f32(params, "attack_ms").map_err(Error::msg)?,
        release_ms: required_f32(params, "release_ms").map_err(Error::msg)?,
        hold_ms: required_f32(params, "hold_ms").map_err(Error::msg)?,
        hysteresis_db: required_f32(params, "hysteresis_db").map_err(Error::msg)?,
    })
}

pub struct BasicNoiseGate {
    /// Linear threshold at which a closed gate opens.
    threshold_open: f32,
    /// Linear threshold at which an open gate closes. Strictly lower than
    /// `threshold_open` whenever `hysteresis_db > 0`. The band between
    /// the two thresholds is the "hysteresis zone" where the gate keeps
    /// whatever state it currently holds, which kills chattering on signals
    /// that bounce around a single threshold point.
    threshold_close: f32,
    /// Maximum samples the gate stays open after the signal drops below
    /// `threshold_close`. Reset to this value every time the envelope rises
    /// back above `threshold_open`. Lets natural note decay through without
    /// prematurely cutting off sustain.
    hold_samples_max: u32,
    /// Countdown of samples remaining before the gate can close. Only
    /// decremented when the gate is open AND the envelope is below
    /// `threshold_close`. Signals in the hysteresis band do not tick the
    /// countdown.
    hold_samples_remaining: u32,
    /// Current logical gate state, independent of the smoothed gain ramp.
    is_open: bool,
    /// Tracks signal level for threshold comparison.
    envelope: EnvelopeFollower,
    /// Smooths the gate gain (0.0 → 1.0) to avoid clicks on threshold crossings.
    gain_follower: EnvelopeFollower,
}

impl BasicNoiseGate {
    pub fn new(
        threshold_db: f32,
        attack_ms: f32,
        release_ms: f32,
        hold_ms: f32,
        hysteresis_db: f32,
        sample_rate: f32,
    ) -> Self {
        let close_db = threshold_db - hysteresis_db.max(0.0);
        let hold_samples_max = ((hold_ms.max(0.0) / 1000.0) * sample_rate).round() as u32;
        Self {
            threshold_open: db_to_lin(threshold_db),
            threshold_close: db_to_lin(close_db),
            hold_samples_max,
            hold_samples_remaining: 0,
            is_open: false,
            envelope: EnvelopeFollower::from_ms(attack_ms, release_ms, sample_rate),
            gain_follower: EnvelopeFollower::from_ms(attack_ms, release_ms, sample_rate),
        }
    }
}

impl MonoProcessor for BasicNoiseGate {
    fn process_sample(&mut self, input: f32) -> f32 {
        let env = self.envelope.process(input.abs());

        if env >= self.threshold_open {
            // Above the open threshold: gate is definitely open, and any
            // pending hold countdown resets to the full duration so we get
            // the full hold after the next below-threshold excursion.
            self.is_open = true;
            self.hold_samples_remaining = self.hold_samples_max;
        } else if env < self.threshold_close {
            // Below the close threshold: start (or continue) counting down
            // the hold window. When it hits zero, close the gate.
            if self.is_open {
                if self.hold_samples_remaining > 0 {
                    self.hold_samples_remaining -= 1;
                } else {
                    self.is_open = false;
                }
            }
        }
        // else: in the hysteresis band (close <= env < open) — keep whatever
        // state we had. No hold-countdown tick here, so a signal that hovers
        // in the band stays gated open indefinitely until it actually drops
        // below the close threshold.

        let target = if self.is_open { 1.0_f32 } else { 0.0_f32 };
        // Smooth the gain transition — eliminates clicks on state changes.
        let gain = self.gain_follower.process(target);
        input * gain
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let params = params_from_set(params)?;
    Ok(Box::new(BasicNoiseGate::new(
        params.threshold,
        params.attack_ms,
        params.release_ms,
        params.hold_ms,
        params.hysteresis_db,
        sample_rate,
    )))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: block_core::AudioChannelLayout,
) -> Result<block_core::BlockProcessor> {
    match layout {
        block_core::AudioChannelLayout::Mono => Ok(block_core::BlockProcessor::Mono(
            build_processor(params, sample_rate)?,
        )),
        block_core::AudioChannelLayout::Stereo => anyhow::bail!(
            "gate model '{}' is mono-only and cannot build native stereo processing",
            MODEL_ID
        ),
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    fn default_params() -> block_core::param::ParameterSet {
        let schema = model_schema();
        block_core::param::ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    #[test]
    fn process_frame_silence_output_is_finite() {
        let params = default_params();
        let mut proc = build_processor(&params, 44100.0).unwrap();
        for i in 0..1024 {
            let out = proc.process_sample(0.0);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
        }
    }

    #[test]
    fn process_frame_sine_output_is_finite() {
        let params = default_params();
        let mut proc = build_processor(&params, 44100.0).unwrap();
        for i in 0..1024 {
            let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            let out = proc.process_sample(input);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
        }
    }

    #[test]
    fn process_block_1024_frames_all_finite() {
        let params = default_params();
        let mut proc = build_processor(&params, 44100.0).unwrap();
        let mut buf: Vec<f32> = (0..1024)
            .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
            .collect();
        proc.process_block(&mut buf);
        for (i, &s) in buf.iter().enumerate() {
            assert!(s.is_finite(), "non-finite at index {i}: {s}");
        }
    }

    #[test]
    fn process_gate_silence_stays_silent() {
        let params = default_params();
        let mut proc = build_processor(&params, 44100.0).unwrap();
        let mut buf = vec![0.0_f32; 1024];
        proc.process_block(&mut buf);
        assert!(
            buf.iter().all(|s| s.abs() < 1e-6),
            "gate should not add energy to silence"
        );
    }

    // ── Helpers for hold + hysteresis tests ─────────────────────────────
    //
    // The envelope follower attack/release is smoothing the gate's view of
    // the instantaneous signal, and the gain_follower smooths the 0↔1
    // transition itself. Both make direct sample-exact assertions fragile
    // on short buffers, so these helpers feed long-enough blocks of
    // constant-level noise that the envelope has time to converge.

    const SR: f32 = 48000.0;

    fn ms_to_samples(ms: f32) -> usize {
        ((ms / 1000.0) * SR) as usize
    }

    fn build_gate(hold_ms: f32, hysteresis_db: f32) -> BasicNoiseGate {
        BasicNoiseGate::new(
            -40.0, // threshold_db — open at -40 dBFS
            1.0,   // attack_ms — fast envelope so tests converge quickly
            5.0,   // release_ms — same
            hold_ms,
            hysteresis_db,
            SR,
        )
    }

    fn drive_constant(gate: &mut BasicNoiseGate, level: f32, samples: usize) -> f32 {
        let mut last = 0.0;
        for _ in 0..samples {
            last = gate.process_sample(level);
        }
        last
    }

    #[test]
    fn hysteresis_keeps_gate_open_in_zone_between_thresholds() {
        // Open threshold -40 dB (~0.01 lin), hysteresis 6 dB → close -46 dB
        // (~0.005 lin). A signal at ~0.007 lin sits inside the hysteresis
        // band: above close, below open. Once opened, the gate must stay
        // open even though the envelope is below the open threshold.
        let mut gate = build_gate(0.0, 6.0);

        // Drive above open threshold to open the gate.
        let opened = drive_constant(&mut gate, 0.1, ms_to_samples(50.0));
        assert!(
            opened.abs() > 0.05,
            "gate did not open on strong signal: out = {opened}"
        );

        // Drop into the hysteresis band and hold.
        let held = drive_constant(&mut gate, 0.007, ms_to_samples(300.0));
        assert!(
            held.abs() > 0.003,
            "gate closed in hysteresis band; expected ~input, got {held}"
        );
    }

    #[test]
    fn hold_keeps_gate_open_during_short_dip_below_close_threshold() {
        // hold_ms = 200 ms, hysteresis 0 dB. After opening, a brief dip
        // (50 ms) below the close threshold must not close the gate —
        // the hold countdown isn't exhausted yet.
        let mut gate = build_gate(200.0, 0.0);

        drive_constant(&mut gate, 0.1, ms_to_samples(50.0));
        // Dip to deep silence for 50 ms — well inside the 200 ms hold.
        drive_constant(&mut gate, 0.0, ms_to_samples(50.0));
        // Come back strong: the next sample should still be gated open.
        let resumed = drive_constant(&mut gate, 0.1, ms_to_samples(20.0));
        assert!(
            resumed.abs() > 0.05,
            "gate closed during hold window: out = {resumed}"
        );
    }

    #[test]
    fn hold_allows_gate_to_close_after_hold_window_elapses() {
        // hold_ms = 50 ms, hysteresis 0 dB. After a 300 ms dip below the
        // close threshold, the hold has long expired; the gate must be
        // closed and output attenuated.
        let mut gate = build_gate(50.0, 0.0);

        drive_constant(&mut gate, 0.1, ms_to_samples(50.0));
        let tail = drive_constant(&mut gate, 0.0, ms_to_samples(300.0));
        assert!(
            tail.abs() < 1e-5,
            "gate did not close after hold expired: out = {tail}"
        );
    }

    #[test]
    fn hold_zero_closes_immediately_matching_old_behavior() {
        // hold_ms = 0, hysteresis 0 dB recovers the pre-issue #300 behavior:
        // gate closes as soon as the envelope drops below threshold.
        let mut gate = build_gate(0.0, 0.0);

        drive_constant(&mut gate, 0.1, ms_to_samples(50.0));
        let tail = drive_constant(&mut gate, 0.0, ms_to_samples(50.0));
        assert!(
            tail.abs() < 1e-5,
            "gate with hold=0 stayed open longer than release settle: out = {tail}"
        );
    }

    #[test]
    fn hysteresis_zero_behaves_like_single_threshold() {
        // hysteresis_db = 0 → close threshold equals open threshold. Once
        // the envelope drops below, the gate should close as soon as the
        // hold window expires (0 ms here, so: immediately).
        let mut gate = build_gate(0.0, 0.0);

        drive_constant(&mut gate, 0.1, ms_to_samples(50.0));
        // Sit just below the threshold — with zero hysteresis, this is
        // "below close", so the gate closes.
        let out = drive_constant(&mut gate, 0.005, ms_to_samples(100.0));
        assert!(
            out.abs() < 0.001,
            "gate with hysteresis=0 should close below threshold: out = {out}"
        );
    }

    #[test]
    fn hold_ms_default_is_backward_compatible() {
        // Presets saved before issue #300 don't carry hold_ms or
        // hysteresis_db. Parsing a full default schema gives hold=150
        // hysteresis=6, which are the "better than old behavior" defaults.
        let gp = GateParams::default();
        assert!((gp.hold_ms - 150.0).abs() < 1e-6, "hold_ms default drift");
        assert!(
            (gp.hysteresis_db - 6.0).abs() < 1e-6,
            "hysteresis_db default drift"
        );
    }
}

pub const MODEL_DEFINITION: DynModelDefinition = DynModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: DynBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
