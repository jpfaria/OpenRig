//! Responsibility: reshapes interleaved samples into stereo frames.

/// Broadcast a mono `f32` buffer into stereo frames where `L == R`.
///
/// Required by the engine's "always-stereo internal bus" invariant
/// (CLAUDE.md invariant 5).
pub fn broadcast_mono_to_stereo(mono: &[f32]) -> Vec<[f32; 2]> {
    mono.iter().map(|&s| [s, s]).collect()
}

/// Convert an interleaved sample buffer into stereo frames, broadcasting
/// mono inputs and pairing up samples for stereo inputs.
///
/// `channels > 2` is collapsed to stereo by taking the first two channels
/// per frame (consistent with how the live rig handles >2-ch devices today).
pub fn interleaved_to_stereo_frames(interleaved: &[f32], channels: u16) -> Vec<[f32; 2]> {
    match channels {
        1 => broadcast_mono_to_stereo(interleaved),
        2 => interleaved.chunks_exact(2).map(|c| [c[0], c[1]]).collect(),
        n => {
            let n = n as usize;
            interleaved.chunks_exact(n).map(|c| [c[0], c[1]]).collect()
        }
    }
}
