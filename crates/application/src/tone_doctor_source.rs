//! Responsibility: picks the Tone Doctor's input signal from the chain's sources.
//! #948 — the owner's order is live guitars, then the looper, then the DI: the
//! first one actually sounding is analysed. The guitars are summed, and so are
//! the playing loops, each tiled to the analysis window.

use std::sync::Arc;

use engine::LoopPcm;

/// One candidate signal: stereo frames and the rate they were captured at.
pub type ToneSignal = (Vec<[f32; 2]>, f32);

/// The first candidate that carries a tone, in the order given. When none does,
/// the first one that exists is returned anyway, so the diagnosis reports the
/// silent window instead of "no source".
pub fn pick_signal(candidates: Vec<Option<ToneSignal>>) -> Option<ToneSignal> {
    let present: Vec<ToneSignal> = candidates.into_iter().flatten().collect();
    let sounding = present
        .iter()
        .position(|(frames, _)| crate::tone_doctor_report::has_usable_signal(frames));
    let index = sounding.unwrap_or(0);
    present.into_iter().nth(index)
}

/// Sum the mono captures of every guitar stream, frame by frame, as stereo.
/// Streams of different lengths sum over the shortest.
pub fn sum_streams(streams: &[Vec<f32>]) -> Vec<[f32; 2]> {
    let frames = streams.iter().map(Vec::len).min().unwrap_or(0);
    (0..frames)
        .map(|i| {
            let s: f32 = streams.iter().map(|stream| stream[i]).sum();
            [s, s]
        })
        .collect()
}

/// Sum every loop (interleaved stereo) into `frames` frames, each loop repeated
/// from its start the way it plays.
pub fn mix_loops<T: AsRef<[f32]>>(loops: &[T], frames: usize) -> Vec<[f32; 2]> {
    let mut mixed = vec![[0.0_f32; 2]; frames];
    for pcm in loops {
        let pcm = pcm.as_ref();
        let len = pcm.len() / 2;
        if len == 0 {
            continue;
        }
        for (i, out) in mixed.iter_mut().enumerate() {
            let at = (i % len) * 2;
            out[0] += pcm[at];
            out[1] += pcm[at + 1];
        }
    }
    mixed
}

/// `seconds` of the playing loops, summed, at their recorded rate — `None` when
/// none plays. The loops of one chain share the store's rate.
pub fn playing_loops_window(loops: &[Arc<LoopPcm>], seconds: usize) -> Option<ToneSignal> {
    let rate = loops.first()?.sample_rate();
    let takes: Vec<&[f32]> = loops.iter().map(|pcm| pcm.samples()).collect();
    Some((mix_loops(&takes, seconds * rate as usize), rate as f32))
}

#[cfg(test)]
#[path = "tone_doctor_source_tests.rs"]
mod tests;
