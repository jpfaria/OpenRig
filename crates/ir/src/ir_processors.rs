//! Responsibility: exposes the convolver as a block processor.

use anyhow::Result;
use block_core::{MonoProcessor, StereoProcessor};

use crate::fft_convolver::FftBlockConvolver;

pub struct MonoIrProcessor {
    convolver: FftBlockConvolver,
}

impl MonoIrProcessor {
    pub fn new(ir: Vec<f32>) -> Result<Self> {
        Ok(Self {
            convolver: FftBlockConvolver::new(ir)?,
        })
    }
}

impl MonoProcessor for MonoIrProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let mut single = [input];
        self.process_block(&mut single);
        single[0]
    }

    fn process_block(&mut self, buffer: &mut [f32]) {
        self.convolver.process_block_in_place(buffer);
    }
}

pub struct StereoIrProcessor {
    left: FftBlockConvolver,
    right: FftBlockConvolver,
}

impl StereoIrProcessor {
    pub fn new(left: Vec<f32>, right: Vec<f32>) -> Result<Self> {
        Ok(Self {
            left: FftBlockConvolver::new(left)?,
            right: FftBlockConvolver::new(right)?,
        })
    }
}

impl StereoProcessor for StereoIrProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mut block = [input];
        self.process_block(&mut block);
        block[0]
    }

    fn process_block(&mut self, buffer: &mut [[f32; 2]]) {
        let mut left = Vec::with_capacity(buffer.len());
        let mut right = Vec::with_capacity(buffer.len());
        for frame in buffer.iter() {
            left.push(frame[0]);
            right.push(frame[1]);
        }
        self.left.process_block_in_place(&mut left);
        self.right.process_block_in_place(&mut right);
        for ((frame, left_sample), right_sample) in buffer
            .iter_mut()
            .zip(left.into_iter())
            .zip(right.into_iter())
        {
            *frame = [left_sample, right_sample];
        }
    }
}
