    use super::*;

    // --- db_to_linear / linear_to_db ---

    #[test]
    fn db_to_linear_zero_db_is_unity() {
        let result = db_to_linear(0.0);
        assert!((result - 1.0).abs() < 1e-6);
    }

    #[test]
    fn db_to_linear_minus_20_is_point_one() {
        let result = db_to_linear(-20.0);
        assert!((result - 0.1).abs() < 1e-6);
    }

    #[test]
    fn db_to_linear_plus_20_is_ten() {
        let result = db_to_linear(20.0);
        assert!((result - 10.0).abs() < 1e-4);
    }

    #[test]
    fn linear_to_db_roundtrip() {
        for db in [-12.0_f32, -6.0, 0.0, 3.0, 6.0, 12.0] {
            let lin = db_to_linear(db);
            let back = linear_to_db(lin);
            assert!((back - db).abs() < 1e-4, "roundtrip failed for {} dB: got {}", db, back);
        }
    }

    // --- freq_to_x / gain_to_y ---

    #[test]
    fn freq_to_x_min_freq_returns_zero() {
        let x = freq_to_x(20.0);
        assert_eq!(x, 0.0);
    }

    #[test]
    fn freq_to_x_max_freq_returns_svg_width() {
        let x = freq_to_x(20_000.0);
        assert_eq!(x, 1000.0);
    }

    #[test]
    fn gain_to_y_zero_db_returns_mid_height() {
        let y = gain_to_y(0.0);
        assert_eq!(y, 100.0); // EQ_SVG_H / 2
    }

    #[test]
    fn gain_to_y_max_gain_returns_zero() {
        let y = gain_to_y(24.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn gain_to_y_min_gain_returns_svg_height() {
        let y = gain_to_y(-24.0);
        assert_eq!(y, 200.0);
    }

    // --- biquad_kind_for_group ---

    #[test]
    fn biquad_kind_low_group_returns_low_shelf() {
        assert!(matches!(biquad_kind_for_group("Low Band"), block_core::BiquadKind::LowShelf));
        assert!(matches!(biquad_kind_for_group("low"), block_core::BiquadKind::LowShelf));
    }

    #[test]
    fn biquad_kind_high_group_returns_high_shelf() {
        assert!(matches!(biquad_kind_for_group("High Band"), block_core::BiquadKind::HighShelf));
        assert!(matches!(biquad_kind_for_group("HIGH"), block_core::BiquadKind::HighShelf));
    }

    #[test]
    fn biquad_kind_mid_group_returns_peak() {
        assert!(matches!(biquad_kind_for_group("Mid"), block_core::BiquadKind::Peak));
        assert!(matches!(biquad_kind_for_group(""), block_core::BiquadKind::Peak));
    }

    // --- eq_frequencies ---

    #[test]
    fn eq_frequencies_returns_200_points() {
        let freqs = eq_frequencies();
        assert_eq!(freqs.len(), 200);
    }

    #[test]
    fn eq_frequencies_starts_at_20_hz_ends_at_20k_hz() {
        let freqs = eq_frequencies();
        assert!((freqs[0] - 20.0).abs() < 0.1);
        assert!((freqs[199] - 20_000.0).abs() < 1.0);
    }

    #[test]
    fn eq_frequencies_monotonically_increasing() {
        let freqs = eq_frequencies();
        for i in 1..freqs.len() {
            assert!(freqs[i] > freqs[i - 1], "freq[{}]={} must be > freq[{}]={}", i, freqs[i], i - 1, freqs[i - 1]);
        }
    }

    // --- db_vec_to_svg_path ---

    #[test]
    fn db_vec_to_svg_path_starts_with_move_command() {
        let dbs = vec![0.0; 200];
        let path = db_vec_to_svg_path(&dbs);
        assert!(path.starts_with("M "), "SVG path should start with M: {}", &path[..20]);
    }

    #[test]
    fn db_vec_to_svg_path_contains_line_commands() {
        let dbs = vec![0.0; 200];
        let path = db_vec_to_svg_path(&dbs);
        assert!(path.contains(" L "), "SVG path should contain L commands");
    }

    #[test]
    fn db_vec_to_svg_path_empty_dbs_returns_empty() {
        let path = db_vec_to_svg_path(&[]);
        assert!(path.is_empty());
    }
