    use super::*;

    fn defaults() -> Settings {
        Settings { drive: 40.0, bias: 50.0, tone: 60.0, level: 50.0 }
    }

    #[test]
    fn fold_silence_in_silence_out() {
        // sin(0) - sin(0) = 0 for any drive.
        for d in [0.5_f32, 1.0, 5.0, 10.0] {
            for b in [-0.3_f32, 0.0, 0.3] {
                assert!(WavefolderProcessor::fold(0.0, d, b).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn fold_is_bounded() {
        for d in [0.5_f32, 1.0, 5.0, 10.0] {
            for x in [-100.0_f32, -1.0, 0.0, 1.0, 100.0] {
                let y = WavefolderProcessor::fold(x, d, 0.0);
                // sin output ∈ [-1, 1], minus a constant offset → [-2, 2].
                assert!(y.abs() <= 2.5, "fold({x}, {d}) = {y}");
            }
        }
    }

    #[test]
    fn high_drive_produces_more_zero_crossings_than_low_drive() {
        // Driving the fold harder cascades more folds → more sign changes
        // per period of the input. We feed a slow-rising ramp and count
        // crossings of the fold output.
        let count_crossings = |drive: f32| {
            let mut prev = 0.0_f32;
            let mut n = 0;
            for i in 0..1000 {
                let x = i as f32 / 1000.0 * 2.0 - 1.0; // -1..+1 ramp
                let y = WavefolderProcessor::fold(x, drive, 0.0);
                if (prev <= 0.0 && y > 0.0) || (prev >= 0.0 && y < 0.0) {
                    n += 1;
                }
                prev = y;
            }
            n
        };
        let low = count_crossings(1.0);
        let high = count_crossings(8.0);
        assert!(high > low, "expected more crossings at high drive: low={low}, high={high}");
    }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = WavefolderProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = WavefolderProcessor::new(defaults(), 44_100.0);
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
