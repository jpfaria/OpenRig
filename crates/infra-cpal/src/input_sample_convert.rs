//! Responsibility: converts a device's integer input samples to f32 in a preallocated buffer.

/// Scratch the integer input arms convert into, owned by one stream callback.
pub(crate) struct InputSampleBuffer {
    samples: Vec<f32>,
}

impl InputSampleBuffer {
    /// `samples`: frames × channels the stream was opened with. Allocated
    /// here, at stream build time, so the callback never has to (invariant #8).
    pub(crate) fn with_capacity(samples: usize) -> Self {
        Self {
            samples: vec![0.0; samples],
        }
    }

    /// `data` converted sample by sample with `to_f32`. No allocation while
    /// the callback delivers at most the frames the stream was opened with.
    pub(crate) fn convert<T: Copy>(&mut self, data: &[T], to_f32: fn(T) -> f32) -> &[f32] {
        self.samples.resize(data.len(), 0.0);
        for (dst, src) in self.samples.iter_mut().zip(data.iter().copied()) {
            *dst = to_f32(src);
        }
        &self.samples
    }
}

pub(crate) fn i16_to_f32(sample: i16) -> f32 {
    sample as f32 / i16::MAX as f32
}

pub(crate) fn u16_to_f32(sample: u16) -> f32 {
    (sample as f32 / u16::MAX as f32) * 2.0 - 1.0
}

pub(crate) fn i32_to_f32(sample: i32) -> f32 {
    sample as f32 / i32::MAX as f32
}

#[cfg(test)]
#[path = "input_sample_convert_tests.rs"]
mod tests;
