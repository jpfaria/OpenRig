//! Responsibility: runs one callback of a split over the segment's bus.
//!
//! Audio-thread hot path (#328, spec §4.1, §11.4): every path runs in the
//! split's own buffer for it, never in another segment or runtime, and the
//! paths run at the same time on the split's lanes. No allocation (a
//! callback larger than the buffers preallocated at build runs in chunks),
//! no lock, no log.

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_split::lanes::PathRun;
use crate::runtime_split::mix::{accumulate_path, finish_mix, path_input};
use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_state::BlockRuntimeNode;

/// Runs the split over the bus in chunks no larger than the paths'
/// preallocated buffers, so an oversized callback never grows them.
pub(crate) fn process_split<F>(split: &mut SplitRuntimeState, frames: &mut [AudioFrame], run: F)
where
    F: Fn(&mut BlockRuntimeNode, &mut [AudioFrame]) + Sync,
{
    let chunk = split
        .bufs
        .iter()
        .map(Vec::capacity)
        .min()
        .unwrap_or(0)
        .max(1);
    for part in frames.chunks_mut(chunk) {
        process_chunk(split, part, &run);
    }
}

/// 1. fill every path's buffer from the bus; 2. run every path through
/// `run`, each on its own lane; 3. delay every path up to the longest;
/// 4. mix back into the bus.
fn process_chunk(split: &mut SplitRuntimeState, frames: &mut [AudioFrame], run: &PathRun<'_>) {
    let SplitRuntimeState {
        mixes,
        paths,
        bufs,
        aligns,
        values,
        knobs,
        lanes,
    } = split;
    let mix = knobs.load_into(*mixes, values);
    for (buf, path) in bufs.iter_mut().zip(values.iter()) {
        buf.clear();
        for frame in frames.iter() {
            buf.push(AudioFrame::Stereo(path_input(stereo(*frame), path, &mix)));
        }
    }
    lanes.run_paths(paths, bufs, aligns, run);
    for (index, frame) in frames.iter_mut().enumerate() {
        let mut acc = [0.0_f32; 2];
        for (buf, path) in bufs.iter().zip(values.iter()) {
            accumulate_path(&mut acc, stereo(buf[index]), path);
        }
        *frame = AudioFrame::Stereo(finish_mix(acc, &mix));
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
