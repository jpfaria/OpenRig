use super::bus_layout_after;
use block_core::{AudioChannelLayout, ModelAudioMode};

#[test]
fn a_true_stereo_block_on_a_mono_bus_leaves_it_stereo() {
    assert_eq!(
        bus_layout_after(ModelAudioMode::TrueStereo, AudioChannelLayout::Mono),
        Some(AudioChannelLayout::Stereo),
        "#992: the mono bus is broadcast to both inputs, never skipped"
    );
}

#[test]
fn every_other_case_is_the_models_own_rule() {
    for mode in [
        ModelAudioMode::MonoOnly,
        ModelAudioMode::DualMono,
        ModelAudioMode::TrueStereo,
        ModelAudioMode::MonoToStereo,
    ] {
        for bus in [AudioChannelLayout::Mono, AudioChannelLayout::Stereo] {
            if matches!(
                (mode, bus),
                (ModelAudioMode::TrueStereo, AudioChannelLayout::Mono)
            ) {
                continue;
            }
            assert_eq!(
                bus_layout_after(mode, bus),
                mode.output_layout(bus),
                "{mode:?} on a {bus:?} bus"
            );
        }
    }
}
