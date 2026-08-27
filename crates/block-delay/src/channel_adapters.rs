//! Responsibility: adapts a mono delay processor to a stereo chain.

use anyhow::Result;
use block_core::{MonoProcessor, StereoProcessor};

pub struct DualMonoProcessor {
    left: Box<dyn MonoProcessor>,
    right: Box<dyn MonoProcessor>,
}

impl DualMonoProcessor {
    pub fn new(left: Box<dyn MonoProcessor>, right: Box<dyn MonoProcessor>) -> Self {
        Self { left, right }
    }
}

impl StereoProcessor for DualMonoProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }
}

pub fn build_dual_mono_from_builder<F>(builder: F) -> Result<Box<dyn StereoProcessor>>
where
    F: Fn() -> Result<Box<dyn MonoProcessor>>,
{
    let left = builder()?;
    let right = builder()?;
    Ok(Box::new(DualMonoProcessor::new(left, right)))
}

/// Runs a stereo processor on a mono host bus: broadcasts the sample to both
/// channels and sums the result. Lets a true-stereo model (e.g. ping-pong)
/// degrade gracefully when the layout is mono.
pub struct StereoToMono {
    inner: Box<dyn StereoProcessor>,
}

impl StereoToMono {
    pub fn new(inner: Box<dyn StereoProcessor>) -> Self {
        Self { inner }
    }
}

impl MonoProcessor for StereoToMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let [l, r] = self.inner.process_frame([input, input]);
        (l + r) * 0.5
    }
}
