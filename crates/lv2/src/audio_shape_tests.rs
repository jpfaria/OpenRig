//! #1105: which audio port counts the LV2 host can run, and with which processor.

use super::{classify, AudioShape};

#[test]
fn mono_and_mono_sidechain_plugins_run_on_the_mono_processor() {
    assert_eq!(classify(1, 1), Some(AudioShape::MonoInput));
    assert_eq!(classify(2, 1), Some(AudioShape::MonoInput));
}

#[test]
fn stereo_output_plugins_run_on_the_stereo_processor() {
    assert_eq!(classify(1, 2), Some(AudioShape::StereoInput));
    assert_eq!(classify(2, 2), Some(AudioShape::StereoInput));
}

#[test]
fn a_stereo_plugin_with_a_sidechain_input_runs_on_the_stereo_processor() {
    // ZamCompX2 / ZamGateX2: L, R, sidechain in; L, R out.
    assert_eq!(classify(3, 2), Some(AudioShape::StereoInput));
}

#[test]
fn shapes_without_a_processor_are_refused() {
    assert_eq!(classify(0, 2), None);
    assert_eq!(classify(4, 2), None);
    assert_eq!(classify(2, 4), None);
}
