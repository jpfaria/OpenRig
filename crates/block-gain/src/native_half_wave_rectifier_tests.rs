    use super::*;

    fn defaults() -> Settings {
        Settings { drive: 50.0, tone: 50.0, octave_mix: 70.0, level: 50.0 }
    }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = OctaveProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = OctaveProcessor::new(defaults(), 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.5;
            let out = p.process_sample(s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 { any_nonzero = true; }
        }
        assert!(any_nonzero);
    }

    #[test]
    fn dc_input_is_blocked() {
        let mut p = OctaveProcessor::new(defaults(), 44_100.0);
        for _ in 0..8192 { let _ = p.process_sample(0.5); }
        let mut peak = 0.0_f32;
        for _ in 0..2048 { peak = peak.max(p.process_sample(0.5).abs()); }
        assert!(peak < 0.05, "DC was not blocked (peak {peak})");
    }

    #[test]
    fn pure_sine_doubles_dominant_frequency() {
        // Feed a low-frequency sine; the rectified output should have
        // most energy at 2× the input frequency. We can't do an FFT in
        // this lightweight test, but we CAN verify the rectified signal
        // crosses zero at twice the rate of the input.
        // Use settings: octave_mix=100% (no dry), drive=0 (no extra fuzz),
        // tone=100% (no LPF smoothing), level=50% (unity).
        let mut p = OctaveProcessor::new(
            Settings { drive: 0.0, tone: 100.0, octave_mix: 100.0, level: 50.0 },
            44_100.0,
        );
        let sr = 44_100.0_f32;
        let f_in = 110.0; // low E ~ 82 Hz, well-tracked
        // Skip warm-up.
        for i in 0..1024 {
            let _ = p.process_sample((2.0 * std::f32::consts::PI * f_in * i as f32 / sr).sin());
        }
        // Count zero crossings over a reasonable window.
        let mut prev = 0.0_f32;
        let mut crossings = 0;
        let window_samples = (sr / 10.0) as usize; // 0.1s
        for i in 0..window_samples {
            let s = (2.0 * std::f32::consts::PI * f_in * (i + 1024) as f32 / sr).sin();
            let out = p.process_sample(s);
            if (prev <= 0.0 && out > 0.0) || (prev >= 0.0 && out < 0.0) {
                crossings += 1;
            }
            prev = out;
        }
        let observed_freq = crossings as f32 / 2.0 / 0.1;
        // We expect ~220 Hz (2× input). Allow 30% tolerance for harmonic
        // content and tracking imperfection.
        assert!(
            (observed_freq - 220.0).abs() < 70.0,
            "expected ~220 Hz, observed {observed_freq:.1}",
        );
    }

    #[test]
    fn dual_mono_produces_finite_output_for_both_channels() {
        let mut dm = DualMonoProcessor {
            left: OctaveProcessor::new(defaults(), 44_100.0),
            right: OctaveProcessor::new(defaults(), 44_100.0),
        };
        for i in 0..1024 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44_100.0).sin() * 0.5;
            let [l, r] = StereoProcessor::process_frame(&mut dm, [s, s]);
            assert!(l.is_finite() && r.is_finite());
        }
    }
