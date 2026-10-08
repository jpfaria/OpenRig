//! Responsibility: maps an LV2 plugin's audio port counts to the processor shape that runs it.

/// Which processor runs a plugin, decided only by how many audio ports it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioShape {
    /// One output: `Lv2Processor`, every audio-in fed by the same buffer.
    MonoInput,
    /// Two outputs: `StereoLv2Processor` (a third input is the sidechain).
    StereoInput,
}

/// `None` when the host has no processor for that port count.
///
/// (2, 1) is a SIDECHAIN plugin (ZaMcomp, ZamGate…): main input plus a
/// detector input, one output. It is mono as far as the chain is concerned —
/// `Lv2Processor` connects every audio-in port to the same buffer, so the
/// detector reads the signal the main input carries (an internal sidechain,
/// which is what the plugin does with its own sidechain switch off).
/// (3, 2) is the stereo form (ZamCompX2, ZamGateX2): L, R and a detector
/// input that `StereoLv2Processor` feeds with the mid of the stereo frame.
pub(crate) fn classify(audio_in: usize, audio_out: usize) -> Option<AudioShape> {
    match (audio_in, audio_out) {
        (1, 1) | (2, 1) => Some(AudioShape::MonoInput),
        (1, 2) | (2, 2) | (3, 2) => Some(AudioShape::StereoInput),
        _ => None,
    }
}

#[cfg(test)]
#[path = "audio_shape_tests.rs"]
mod tests;
