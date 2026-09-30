use super::*;

#[test]
fn an_untouched_mix_is_unity_and_serializes_empty() {
    let mix = ChainMix::default();
    assert!(mix.is_unity());
    assert_eq!(serde_json::to_string(&mix).unwrap(), "{}");
}

#[test]
fn endpoint_mut_creates_the_fader_at_unity_then_prune_forgets_it() {
    let mut mix = ChainMix::default();
    mix.endpoint_mut(MixerDirection::Input, "main", "Guitar")
        .gain_db = -6.0;
    assert_eq!(
        mix.endpoint(MixerDirection::Input, "main", "Guitar")
            .map(|e| e.gain_db),
        Some(-6.0)
    );
    assert!(mix
        .endpoint(MixerDirection::Output, "main", "Guitar")
        .is_none());
    mix.endpoint_mut(MixerDirection::Input, "main", "Guitar")
        .gain_db = 0.0;
    mix.prune();
    assert!(mix.is_unity());
}

#[test]
fn a_muted_endpoint_at_unity_survives_prune() {
    let mut mix = ChainMix::default();
    mix.endpoint_mut(MixerDirection::Output, "main", "Main")
        .muted = true;
    mix.prune();
    assert_eq!(mix.endpoints.len(), 1);
}

#[test]
fn a_legacy_chain_without_mix_reads_as_unity() {
    let mix: ChainMix = serde_json::from_str("{}").unwrap();
    assert_eq!(mix, ChainMix::default());
}
