    use super::*;
    use std::f32::consts::TAU;

    fn sr() -> f32 {
        48_000.0
    }

    fn default_limiter() -> BrickWallLimiterMono {
        BrickWallLimiterMono::new(LimiterParams::default(), sr())
    }

    #[test]
    fn silence_produces_silence() {
        let mut lim = default_limiter();
        for _ in 0..1024 {
            let out = lim.process_sample(0.0);
            assert!(out.abs() < 1e-5, "non-silent output {out}");
        }
    }

    #[test]
    fn sine_below_threshold_passes_through() {
        // Default threshold -1 dB. A sine at 0.5 amplitude is ~-6 dB, well below.
        let mut lim = default_limiter();
        // Skip the first lookahead_ms so the delay line has content.
        let warmup = (LimiterParams::default().lookahead_ms * 0.001 * sr()) as usize + 1;
        let mut max_out: f32 = 0.0;
        for i in 0..(warmup + 2048) {
            let x = (i as f32 / sr() * 440.0 * TAU).sin() * 0.5;
            let out = lim.process_sample(x);
            if i >= warmup {
                max_out = max_out.max(out.abs());
            }
        }
        assert!(
            max_out > 0.45 && max_out <= 0.5 + 1e-5,
            "below-threshold signal should pass untouched, max_out={max_out}"
        );
    }

    #[test]
    fn hot_dc_output_below_ceiling() {
        // Hard constant 2.0 — well above threshold (which is linear ~0.89).
        let mut lim = default_limiter();
        let ceiling = lim.ceiling_lin();
        for i in 0..4096 {
            let out = lim.process_sample(2.0);
            assert!(
                out.abs() <= ceiling + 1e-5,
                "sample {i}: {} > ceiling {ceiling}",
                out.abs()
            );
        }
    }

    #[test]
    fn isolated_transient_stays_below_ceiling() {
        // A single hot impulse surrounded by silence must not peek above the
        // ceiling. The clamp() exists as a safety net but the peak should be
        // anticipated by lookahead and reduced *before* the spike arrives.
        let mut lim = default_limiter();
        let ceiling = lim.ceiling_lin();
        // Warmup with silence so gain is at unity.
        for _ in 0..1024 {
            let _ = lim.process_sample(0.0);
        }
        // One hot sample; then long silence to read everything out.
        let mut observed: Vec<f32> = Vec::new();
        observed.push(lim.process_sample(3.0));
        for _ in 0..2048 {
            observed.push(lim.process_sample(0.0));
        }
        let peak_out = observed.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
        assert!(
            peak_out <= ceiling + 1e-4,
            "isolated transient peak {peak_out} above ceiling {ceiling}"
        );
    }

    #[test]
    fn process_block_matches_sample_loop() {
        let mut a = default_limiter();
        let mut b = default_limiter();
        let input: Vec<f32> = (0..1024)
            .map(|i| (i as f32 / sr() * 220.0 * TAU).sin() * 1.5)
            .collect();
        let mut a_out = Vec::with_capacity(input.len());
        for &x in &input {
            a_out.push(a.process_sample(x));
        }
        let mut b_buf = input.clone();
        b.process_block(&mut b_buf);
        for (i, (x, y)) in a_out.iter().zip(b_buf.iter()).enumerate() {
            assert!(
                (x - y).abs() < 1e-6,
                "block vs sample loop diverge at {i}: {x} vs {y}"
            );
        }
    }
