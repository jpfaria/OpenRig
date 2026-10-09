//! #1105: a plugin that only processes blocks of one size (ZamVerb's
//! convolver passes the input through on any other `run()` length) is fed
//! exactly that size, whatever the callback hands the chain.

use super::FixedBlock;
use block_core::StereoProcessor;

/// Doubles the signal and records the length of every block it was given.
struct Recorder {
    lengths: Vec<usize>,
    latency: usize,
}

impl StereoProcessor for Recorder {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.lengths.push(1);
        [input[0] * 2.0, input[1] * 2.0]
    }

    fn process_block(&mut self, buffer: &mut [[f32; 2]]) {
        self.lengths.push(buffer.len());
        for frame in buffer {
            *frame = [frame[0] * 2.0, frame[1] * 2.0];
        }
    }

    fn latency_samples(&self) -> usize {
        self.latency
    }
}

fn recorder(latency: usize) -> Recorder {
    Recorder {
        lengths: Vec::new(),
        latency,
    }
}

fn ramp(len: usize) -> Vec<[f32; 2]> {
    (0..len).map(|i| [i as f32, -(i as f32)]).collect()
}

#[test]
fn the_plugin_only_ever_sees_blocks_of_the_fixed_size() {
    let mut fixed = FixedBlock::new(recorder(0), 64);
    let mut signal = ramp(1000);
    for chunk_len in [100, 37, 1, 300, 64, 498] {
        let (chunk, rest) = signal.split_at_mut(chunk_len);
        fixed.process_block(chunk);
        signal = rest.to_vec();
    }
    let lengths = &fixed.inner().lengths;
    assert_eq!(
        lengths.len(),
        1000 / 64,
        "one run per full block: {lengths:?}"
    );
    assert!(lengths.iter().all(|&len| len == 64), "{lengths:?}");
}

#[test]
fn the_output_is_the_processed_input_one_block_late() {
    let mut fixed = FixedBlock::new(recorder(0), 64);
    let input = ramp(400);
    let mut output = input.clone();
    for chunk in output.chunks_mut(48) {
        fixed.process_block(chunk);
    }
    for (i, frame) in output.iter().enumerate() {
        let expected = if i < 64 {
            [0.0, 0.0]
        } else {
            [input[i - 64][0] * 2.0, input[i - 64][1] * 2.0]
        };
        assert_eq!(*frame, expected, "frame {i}");
    }
}

#[test]
fn frame_by_frame_processing_goes_through_the_same_blocks() {
    let mut fixed = FixedBlock::new(recorder(0), 64);
    let outputs: Vec<[f32; 2]> = ramp(130)
        .into_iter()
        .map(|frame| fixed.process_frame(frame))
        .collect();
    assert!(fixed.inner().lengths.iter().all(|&len| len == 64));
    assert_eq!(fixed.inner().lengths.len(), 2);
    assert_eq!(outputs[63], [0.0, 0.0]);
    assert_eq!(outputs[64], [0.0, 0.0], "input frame 0 doubled");
    assert_eq!(outputs[65], [2.0, -2.0], "input frame 1 doubled");
}

#[test]
fn the_reported_latency_adds_the_block() {
    let fixed = FixedBlock::new(recorder(10), 64);
    assert_eq!(fixed.latency_samples(), 74);
}
