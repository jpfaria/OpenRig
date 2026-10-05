//! #1007: the MASTER and LOOPER faders speak dB; the chain stores a volume
//! percentage and the looper a linear mix.

use domain::mixer_gain::GAIN_DB_MIN;

use super::*;

#[test]
fn unity_volume_is_zero_db_both_ways() {
    assert_eq!(volume_db(100.0), 0.0);
    assert_eq!(volume_percent(0.0), 100);
}

#[test]
fn half_volume_reads_minus_six_db() {
    assert_eq!(volume_db(50.0), -6.0);
    assert_eq!(volume_percent(-6.0), 50);
}

#[test]
fn the_volume_never_passes_its_200_percent_ceiling() {
    assert_eq!(volume_percent(12.0), 200);
}

#[test]
fn the_bottom_of_the_fader_is_silence() {
    assert_eq!(volume_db(0.0), GAIN_DB_MIN);
    assert_eq!(volume_percent(GAIN_DB_MIN), 0);
    assert_eq!(looper_mix_db(0.0), GAIN_DB_MIN);
    assert_eq!(looper_mix(GAIN_DB_MIN), 0.0);
}

#[test]
fn a_full_looper_mix_is_zero_db_and_the_mix_never_passes_one() {
    assert_eq!(looper_mix_db(1.0), 0.0);
    assert_eq!(looper_mix(0.0), 1.0);
    assert_eq!(looper_mix(6.0), 1.0);
}

#[test]
fn half_looper_mix_reads_minus_six_db() {
    assert_eq!(looper_mix_db(0.5), -6.0);
    assert!((looper_mix(-6.0) - 0.501).abs() < 0.001);
}
