    use super::*;

    #[test]
    fn freq_to_bin_returns_zero_for_dc() {
        assert_eq!(freq_to_bin(0.0, 48_000.0), 0);
    }

    #[test]
    fn freq_to_bin_clamps_at_nyquist_minus_one() {
        // Above Nyquist, the result must clamp to FFT_SIZE/2 - 1.
        let bin = freq_to_bin(48_000.0, 48_000.0);
        assert_eq!(bin, FFT_SIZE / 2 - 1);
    }

    #[test]
    fn band_bin_range_is_strictly_increasing() {
        // Each band's hi index must be strictly greater than its lo index.
        for i in 0..N_BANDS {
            let (lo, hi) = band_bin_range(i, 48_000.0);
            assert!(hi > lo, "band {} has empty range [{lo}, {hi})", i);
        }
    }

    #[test]
    fn analyzer_silence_produces_floor_levels() {
        let mut analyzer = SpectrumAnalyzer::new(48_000.0);
        let buffer = vec![0.0_f32; FFT_SIZE];
        let snap = analyzer.process(&buffer);
        // -80 dBFS floor → exactly 0.0 after clamp on every band.
        for &lv in &snap.levels {
            assert_eq!(lv, 0.0);
        }
        for &pk in &snap.peaks {
            assert_eq!(pk, 0.0);
        }
    }

    #[test]
    fn analyzer_sine_at_1khz_lights_the_1k_band() {
        let sample_rate = 48_000.0_f32;
        let freq = 1000.0_f32;
        let buffer: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                let t = i as f32 / sample_rate;
                (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect();

        let mut analyzer = SpectrumAnalyzer::new(sample_rate);
        let snap = analyzer.process(&buffer);

        // The "1k" label is at index 34 (1016 Hz center, 1/6-octave wide).
        let one_k_idx = BAND_LABELS
            .iter()
            .position(|&l| l == "1k")
            .expect("1k label exists");
        let one_k_level = snap.levels[one_k_idx];
        // The 1 kHz tone should produce a strong reading on or near that band.
        // Allow some leakage into neighbours because of the Hann window.
        assert!(
            one_k_level > 0.5,
            "expected level > 0.5 at 1k band, got {one_k_level}"
        );
    }

    #[test]
    fn band_freqs_and_labels_are_aligned() {
        assert_eq!(BAND_FREQS.len(), N_BANDS);
        assert_eq!(BAND_LABELS.len(), N_BANDS);
    }

    #[test]
    fn snapshot_arrays_have_n_bands_size() {
        let mut analyzer = SpectrumAnalyzer::new(48_000.0);
        let snap = analyzer.process(&vec![0.0; FFT_SIZE]);
        assert_eq!(snap.levels.len(), N_BANDS);
        assert_eq!(snap.peaks.len(), N_BANDS);
    }
