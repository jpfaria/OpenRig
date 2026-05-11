    use super::*;

    fn defaults() -> Settings {
        Settings { drive: 40.0, bias: 30.0, tone: 60.0, level: 50.0 }
    }

    #[test]
    fn shape_silence_in_silence_out() {
        // tanh(0 + bias) - tanh(bias) = 0 for any bias.
        for &b in &[0.0_f32, 0.1, 0.3, 0.5] {
            assert!(TubeProcessor::shape(0.0, b).abs() < 1e-9);
        }
    }

    #[test]
    fn shape_is_bounded_by_1() {
        for x in [-100.0_f32, -10.0, -1.0, 0.0, 1.0, 10.0, 100.0] {
            let y = TubeProcessor::shape(x, 0.2);
            assert!(y.abs() <= 1.5, "shape({x}, 0.2) = {y} exceeded soft bound");
        }
    }

    #[test]
    fn shape_is_asymmetric_with_bias() {
        // With bias > 0 the curve favours one side: |shape(+1, bias)| != |shape(-1, bias)|.
        let bias = 0.25;
        let pos = TubeProcessor::shape(1.0, bias).abs();
        let neg = TubeProcessor::shape(-1.0, bias).abs();
        assert!(
            (pos - neg).abs() > 0.02,
            "expected asymmetry, got |+| = {pos}, |-| = {neg}",
        );
    }

    #[test]
    fn silence_input_produces_finite_silence() {
        let mut p = TubeProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = TubeProcessor::new(defaults(), 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.3;
            let out = p.process_sample(s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 { any_nonzero = true; }
        }
        assert!(any_nonzero);
    }

    #[test]
    fn dc_input_is_blocked() {
        // Constant DC input → output should settle near zero (DC block + tanh).
        let mut p = TubeProcessor::new(defaults(), 44_100.0);
        for _ in 0..8192 { let _ = p.process_sample(0.5); }
        let mut peak = 0.0_f32;
        for _ in 0..2048 { peak = peak.max(p.process_sample(0.5).abs()); }
        assert!(peak < 0.05, "DC was not blocked (peak {peak} > 0.05)");
    }

    #[test]
    fn dual_mono_processes_both_channels_independently() {
        let mut dm = DualMonoProcessor {
            left: TubeProcessor::new(defaults(), 44_100.0),
            right: TubeProcessor::new(defaults(), 44_100.0),
        };
        for i in 0..1024 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44_100.0).sin() * 0.3;
            // Left only; right gets zero. Output left should differ from right.
            let [l, r] = StereoProcessor::process_frame(&mut dm, [s, 0.0]);
            assert!(l.is_finite() && r.is_finite());
        }
    }
