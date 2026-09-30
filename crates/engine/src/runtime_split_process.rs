//! Responsibility: runs one callback of a split over the segment's bus.
//!
//! Audio-thread hot path (#328, spec §4.1): path B runs in the split's own
//! buffer, never in another segment or runtime. No allocation (a callback
//! larger than the buffer preallocated at build runs in chunks), no lock,
//! no log.

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_split::mix::{mix_frame, split_inputs};
use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_state::BlockRuntimeNode;

/// Runs the split over the bus in chunks no larger than path B's
/// preallocated buffer, so an oversized callback never grows it.
pub(crate) fn process_split<F>(split: &mut SplitRuntimeState, frames: &mut [AudioFrame], mut run: F)
where
    F: FnMut(&mut BlockRuntimeNode, &mut [AudioFrame]),
{
    let chunk = split.b_buf.capacity().max(1);
    for part in frames.chunks_mut(chunk) {
        process_chunk(split, part, &mut run);
    }
}

/// 1. fill path B's buffer from the bus and feed path A's input to the bus
///    in place; 2. run both paths through `run`; 3. delay the shorter path;
/// 4. mix back into the bus.
fn process_chunk<F>(split: &mut SplitRuntimeState, frames: &mut [AudioFrame], run: &mut F)
where
    F: FnMut(&mut BlockRuntimeNode, &mut [AudioFrame]),
{
    let SplitRuntimeState {
        mixes,
        a,
        b,
        b_buf,
        align_a,
        align_b,
        knobs,
    } = split;
    let values = knobs.load(*mixes);
    b_buf.clear();
    for frame in frames.iter_mut() {
        let (to_a, to_b) = split_inputs(stereo(*frame), &values);
        *frame = AudioFrame::Stereo(to_a);
        b_buf.push(AudioFrame::Stereo(to_b));
    }
    for node in a.iter_mut() {
        run(node, frames);
    }
    for node in b.iter_mut() {
        run(node, b_buf.as_mut_slice());
    }
    align_a.process(frames);
    align_b.process(b_buf.as_mut_slice());
    for (frame, path_b) in frames.iter_mut().zip(b_buf.iter()) {
        *frame = AudioFrame::Stereo(mix_frame(stereo(*frame), stereo(*path_b), &values));
    }
}

#[inline(always)]
fn stereo(frame: AudioFrame) -> [f32; 2] {
    match frame {
        AudioFrame::Mono(sample) => [sample, sample],
        AudioFrame::Stereo(pair) => pair,
    }
}

#[cfg(test)]
#[path = "runtime_split_process_tests.rs"]
mod tests;
