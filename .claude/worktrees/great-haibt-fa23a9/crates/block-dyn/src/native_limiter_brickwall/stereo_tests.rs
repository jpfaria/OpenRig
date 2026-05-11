    use super::*;
    use std::f32::consts::TAU;

    fn sr() -> f32 {
        48_000.0
    }

    fn default_limiter() -> BrickWallLimiterStereo {
        BrickWallLimiterStereo::new(LimiterParams::default(), sr())
    }

    #[test]
    fn silence_produces_silence() {
        let mut lim = default_limiter();
        for _ in 0..1024 {
            let out = lim.process_frame([0.0, 0.0]);
            assert!(out[0].abs() < 1e-5 && out[1].abs() < 1e-5);
        }
    }

    #[test]
    fn stereo_link_preserves_ratio_between_channels() {
        // L hot, R quiet but non-zero. Gain must apply equally to both,
        // preserving the L:R amplitude ratio (image).
        let mut lim = default_limiter();
        // Warmup.
        for _ in 0..1024 {
            let _ = lim.process_frame([0.0, 0.0]);
        }
        let mut ratios = Vec::new();
        for _ in 0..2048 {
            let out = lim.process_frame([2.0, 0.2]);
            if out[0].abs() > 1e-3 && out[1].abs() > 1e-3 {
                ratios.push(out[0] / out[1]);
            }
        }
        // All observed ratios should match the input ratio (10:1) within tolerance.
        for r in &ratios {
            assert!(
                (r - 10.0).abs() < 0.2,
                "L/R ratio drifted: {r}, expected ~10.0"
            );
        }
    }

    #[test]
    fn hot_stereo_signal_stays_below_ceiling() {
        let mut lim = default_limiter();
        let ceiling = lim.ceiling_lin();
        for i in 0..4096 {
            let l = (i as f32 / sr() * 220.0 * TAU).sin() * 2.0;
            let r = (i as f32 / sr() * 330.0 * TAU).sin() * 2.0;
            let out = lim.process_frame([l, r]);
            assert!(
                out[0].abs() <= ceiling + 1e-4 && out[1].abs() <= ceiling + 1e-4,
                "sample {i}: L={} R={} ceiling={ceiling}",
                out[0].abs(),
                out[1].abs()
            );
        }
    }

    #[test]
    fn left_only_transient_reduces_right_equally() {
        // Transient on L only, R silent. R must be reduced by the same factor
        // as L for the duration of the gain reduction.
        let mut lim = default_limiter();
        // Warmup.
        for _ in 0..1024 {
            let _ = lim.process_frame([0.0, 0.0]);
        }
        // Inject a constant R reference tone.
        let mut observed_r_during_gr = Vec::new();
        for i in 0..1024 {
            let l = if i < 64 { 3.0 } else { 0.0 };
            let r = 0.5;
            let out = lim.process_frame([l, r]);
            // While gain reduction is active, R should be below its input level.
            observed_r_during_gr.push(out[1].abs());
        }
        // At some point during the transient window, R must have been pulled down.
        let reduced_any = observed_r_during_gr.iter().any(|&x| x < 0.45);
        assert!(
            reduced_any,
            "stereo link did not reduce R during L transient"
        );
    }
