    use super::*;

    fn defaults() -> Settings {
        Settings { bits: 50.0, rate_pct: 50.0, mix: 100.0 }
    }

    #[test]
    fn quantise_one_bit_collapses_to_two_levels() {
        // 1 bit → 2 levels → step = 1.0 → values round to ±1 or 0.
        let mut seen = std::collections::HashSet::new();
        for i in 0..1000 {
            let x = (i as f32 / 999.0) * 2.0 - 1.0; // -1..+1
            let q = BitcrusherProcessor::quantise(x, 1.0);
            // round-to-nearest with step 1.0 gives -1, 0, or 1.
            seen.insert((q * 1000.0) as i32);
        }
        assert!(seen.len() <= 5, "expected ~3 levels at 1 bit, got {seen:?}");
    }

    #[test]
    fn quantise_16_bit_matches_input_closely() {
        for x in [-0.7_f32, -0.1, 0.0, 0.05, 0.3, 0.99] {
            let q = BitcrusherProcessor::quantise(x, 16.0);
            assert!((q - x).abs() < 1.0 / 32_768.0, "16-bit quant: {x} → {q}");
        }
    }

    #[test]
    fn sample_and_hold_at_50_percent_repeats_every_other_sample() {
        // rate_pct = 50% with default 100% mix and 16 bits (low quant).
        let mut p = BitcrusherProcessor::new(
            Settings { bits: 100.0, rate_pct: 50.0, mix: 100.0 },
            44_100.0,
        );
        // Feed alternating values; SRR should hold each across two output samples.
        let mut last = None;
        let mut held_count = 0;
        for i in 0..40 {
            let v = if i % 2 == 0 { 0.5 } else { -0.5 };
            let out = p.process_sample(v);
            if let Some(prev) = last {
                if (out - prev as f32).abs() < 1e-3 {
                    held_count += 1;
                }
            }
            last = Some(out);
        }
        // We should see consecutive equal pairs for at least half the samples.
        assert!(held_count > 10, "expected sample-and-hold repeats, saw {held_count}");
    }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = BitcrusherProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = BitcrusherProcessor::new(defaults(), 44_100.0);
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
    fn mix_zero_passes_dry_input_through() {
        let mut p = BitcrusherProcessor::new(
            Settings { bits: 50.0, rate_pct: 50.0, mix: 0.0 },
            44_100.0,
        );
        for i in 0..512 {
            let s = i as f32 * 0.001;
            let out = p.process_sample(s);
            // mix=0 → out should equal input (within float tolerance).
            assert!((out - s).abs() < 1e-6);
        }
    }
