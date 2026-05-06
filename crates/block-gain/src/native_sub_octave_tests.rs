    use super::*;

    fn defaults() -> Settings { Settings { sub_mix: 70.0, tone: 40.0, level: 50.0 } }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = SubProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_produces_finite_output() {
        let mut p = SubProcessor::new(defaults(), 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin() * 0.5;
            let out = p.process_sample(s);
            assert!(out.is_finite());
        }
    }

    #[test]
    fn pure_sine_halves_the_dominant_frequency() {
        // Feed a pure sine at f and verify the sub-octave output crosses
        // zero at half the rate (i.e. one period per two input periods).
        // Use sub_mix = 100% (no dry leakage), tone = 100% (raw square),
        // level = 50% (unity-ish).
        let mut p = SubProcessor::new(
            Settings { sub_mix: 100.0, tone: 100.0, level: 50.0 },
            44_100.0,
        );
        let sr = 44_100.0_f32;
        let f_in = 220.0; // → expect 110 Hz output
        // Skip warm-up.
        for i in 0..2048 {
            let _ = p.process_sample((2.0 * std::f32::consts::PI * f_in * i as f32 / sr).sin() * 0.5);
        }
        // Count zero crossings over a measurement window.
        let mut prev = 0.0_f32;
        let mut crossings = 0;
        let window = (sr * 0.2) as usize; // 200 ms
        for i in 0..window {
            let s = (2.0 * std::f32::consts::PI * f_in * (i + 2048) as f32 / sr).sin() * 0.5;
            let out = p.process_sample(s);
            if (prev <= 0.0 && out > 0.0) || (prev >= 0.0 && out < 0.0) {
                crossings += 1;
            }
            prev = out;
        }
        let observed = crossings as f32 / 2.0 / 0.2;
        // Expect ~110 Hz; allow 30% tolerance for transient and edge cases.
        assert!(
            (observed - 110.0).abs() < 35.0,
            "expected ~110 Hz, observed {observed:.1} Hz",
        );
    }

    #[test]
    fn tone_zero_smooths_the_square() {
        // tone=0 → output goes through full LPF, peak should be lower
        // than tone=100% on the same input (since LPF kills harmonics).
        let sr = 44_100.0_f32;
        let f_in = 220.0;
        let make = |tone: f32| {
            let mut p = SubProcessor::new(
                Settings { sub_mix: 100.0, tone, level: 50.0 },
                sr,
            );
            for i in 0..2048 {
                let _ = p.process_sample((2.0 * std::f32::consts::PI * f_in * i as f32 / sr).sin() * 0.5);
            }
            let mut peak = 0.0_f32;
            for i in 0..2048 {
                let s = (2.0 * std::f32::consts::PI * f_in * (i + 2048) as f32 / sr).sin() * 0.5;
                peak = peak.max(p.process_sample(s).abs());
            }
            peak
        };
        let raw = make(100.0);
        let smooth = make(0.0);
        assert!(smooth < raw, "expected smooth ({smooth}) < raw ({raw})");
    }
