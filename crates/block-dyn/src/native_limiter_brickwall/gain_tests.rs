    use super::*;

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn db_round_trip() {
        for db in [-60.0_f32, -12.0, -1.0, 0.0, 3.0, 6.0] {
            let round = lin_to_db(db_to_lin(db));
            assert!(approx(round, db, 1e-3), "db={db} round={round}");
        }
    }

    #[test]
    fn below_knee_produces_no_reduction() {
        assert_eq!(target_gr_db(-20.0, -1.0, 2.0), 0.0);
        assert_eq!(target_gr_db(-2.5, -1.0, 2.0), 0.0); // exactly at knee_low
    }

    #[test]
    fn above_knee_gives_full_reduction() {
        let gr = target_gr_db(5.0, -1.0, 2.0);
        assert!(approx(gr, -6.0, 1e-5), "gr={gr}");
    }

    #[test]
    fn knee_region_is_smooth() {
        // Knee from -2 to 0 around threshold -1, width 2.
        let samples: Vec<f32> = (0..21)
            .map(|i| -2.0 + (i as f32) * 0.1)
            .map(|db| target_gr_db(db, -1.0, 2.0))
            .collect();
        // Monotonically non-increasing.
        for w in samples.windows(2) {
            assert!(w[1] <= w[0] + 1e-5, "non-monotonic: {w:?}");
        }
        // Endpoints match.
        assert!(approx(samples[0], 0.0, 1e-5));
    }

    #[test]
    fn hard_knee_zero_width() {
        assert_eq!(target_gr_db(-2.0, -1.0, 0.0), 0.0);
        assert!(approx(target_gr_db(0.0, -1.0, 0.0), -1.0, 1e-5));
    }

    #[test]
    fn tick_attack_is_instant() {
        let cfg = GainConfig::new(-1.0, 2.0, 100.0, 48_000.0);
        let mut gc = GainComputer::new();
        // Peak well above threshold → target is -5 dB
        let _ = gc.tick(db_to_lin(4.0), &cfg);
        let expected_gr = target_gr_db(4.0, -1.0, 2.0);
        assert!(
            approx(gc.current_gr_db(), expected_gr, 1e-4),
            "gr={} expected={}",
            gc.current_gr_db(),
            expected_gr
        );
    }

    #[test]
    fn tick_release_approaches_unity() {
        let cfg = GainConfig::new(-1.0, 2.0, 50.0, 48_000.0);
        let mut gc = GainComputer::new();
        // Force a large gain reduction.
        let _ = gc.tick(db_to_lin(10.0), &cfg);
        let start_gr = gc.current_gr_db();
        assert!(start_gr < -5.0, "expected heavy reduction, got {start_gr}");
        // After 4 release time constants (≈200 ms at 50 ms release), remaining
        // reduction should be below 0.05 dB for any reasonable starting point.
        let samples = (0.200 * 48_000.0) as usize;
        for _ in 0..samples {
            let _ = gc.tick(0.0, &cfg);
        }
        assert!(
            gc.current_gr_db() > -0.05,
            "expected recovery, got {}",
            gc.current_gr_db()
        );
    }

    #[test]
    fn tick_release_coef_respects_configured_time() {
        // Release ≈ 100 ms: at 48 kHz, after 100 ms we expect ~90% recovery
        // toward 0 dB from the starting GR.
        let cfg = GainConfig::new(-1.0, 0.0, 100.0, 48_000.0);
        let mut gc = GainComputer::new();
        let _ = gc.tick(db_to_lin(10.0), &cfg); // attack to -11 dB
        let start = gc.current_gr_db();
        let samples = (0.100 * 48_000.0) as usize;
        for _ in 0..samples {
            let _ = gc.tick(0.0, &cfg);
        }
        // Should have recovered at least 85% of the distance.
        let recovered = gc.current_gr_db() - start;
        let target_distance = 0.0 - start;
        let fraction = recovered / target_distance;
        assert!(
            fraction > 0.85 && fraction <= 1.0,
            "release recovery fraction {fraction} out of [0.85, 1.0]"
        );
    }

    #[test]
    fn tick_respects_threshold() {
        let cfg = GainConfig::new(-1.0, 0.0, 100.0, 48_000.0);
        let mut gc = GainComputer::new();
        // Peak below threshold → no reduction.
        for _ in 0..100 {
            let g = gc.tick(db_to_lin(-6.0), &cfg);
            assert!(approx(g, 1.0, 1e-4), "g={g} expected 1.0 below threshold");
        }
    }
