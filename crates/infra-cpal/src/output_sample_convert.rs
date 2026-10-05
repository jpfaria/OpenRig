//! Responsibility: converts f32 output samples to a device's integer sample format.

pub(crate) fn f32_to_i16(sample: f32) -> i16 {
    (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

pub(crate) fn f32_to_u16(sample: f32) -> u16 {
    ((sample + 1.0) * 0.5 * u16::MAX as f32).clamp(0.0, u16::MAX as f32) as u16
}

pub(crate) fn f32_to_i32(sample: f32) -> i32 {
    (sample * i32::MAX as f32).clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

/// Preallocated f32 scratch that lets an f32 render feed an integer output
/// buffer without allocating on the audio thread (invariant #8).
pub(crate) struct NativeOutputBuffer {
    scratch: Vec<f32>,
}

impl NativeOutputBuffer {
    pub(crate) fn with_capacity(samples: usize) -> Self {
        Self {
            scratch: vec![0.0; samples],
        }
    }

    /// Renders into the scratch from silence, then converts it into `out`.
    /// Samples past the preallocation are silence.
    pub(crate) fn fill<T: Copy, R: FnMut(&mut [f32]) + ?Sized>(
        &mut self,
        out: &mut [T],
        convert: fn(f32) -> T,
        render: &mut R,
    ) {
        let len = out.len().min(self.scratch.len());
        let scratch = &mut self.scratch[..len];
        scratch.fill(0.0);
        render(scratch);
        for (dst, src) in out.iter_mut().zip(scratch.iter()) {
            *dst = convert(*src);
        }
        out[len..].fill(convert(0.0));
    }
}

#[cfg(test)]
#[path = "output_sample_convert_tests.rs"]
mod tests;
