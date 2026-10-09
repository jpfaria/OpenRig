//! Responsibility: feeds a stereo processor in blocks of one fixed size.
//!
//! Some plugins process nothing unless `run()` gets exactly the block size
//! the host announced (ZamVerb's convolver passes the input through on any
//! other length, #1105), while the callback hands the chain whatever the
//! device delivers. Frames collect in one block while the previous block's
//! output plays out, so the plugin always sees full blocks and the output
//! comes out exactly one block late. Both blocks are allocated at build:
//! nothing is allocated on the audio thread.
use block_core::StereoProcessor;

pub struct FixedBlock<P: StereoProcessor> {
    inner: P,
    /// Frames collected for the next `run()`.
    collecting: Vec<[f32; 2]>,
    /// The last processed block, playing out frame by frame.
    playing: Vec<[f32; 2]>,
    position: usize,
}

impl<P: StereoProcessor> FixedBlock<P> {
    pub fn new(inner: P, block_length: usize) -> Self {
        let block_length = block_length.max(1);
        Self {
            inner,
            collecting: vec![[0.0; 2]; block_length],
            playing: vec![[0.0; 2]; block_length],
            position: 0,
        }
    }

    #[cfg(test)]
    pub fn inner(&self) -> &P {
        &self.inner
    }
}

impl<P: StereoProcessor> StereoProcessor for FixedBlock<P> {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let output = self.playing[self.position];
        self.collecting[self.position] = input;
        self.position += 1;
        if self.position == self.collecting.len() {
            self.inner.process_block(&mut self.collecting);
            std::mem::swap(&mut self.collecting, &mut self.playing);
            self.position = 0;
        }
        output
    }

    fn process_block(&mut self, buffer: &mut [[f32; 2]]) {
        for frame in buffer {
            *frame = self.process_frame(*frame);
        }
    }

    fn latency_samples(&self) -> usize {
        self.inner.latency_samples() + self.collecting.len()
    }
}

#[cfg(test)]
#[path = "fixed_block_tests.rs"]
mod tests;
