//! Stereo VST3 processor: wraps `Vst3Plugin` and implements `StereoProcessor`.

use crate::host::Vst3Plugin;
use block_core::StereoProcessor;

/// Internal processing block size for VST3 plugins (matches OpenRig default).
const BLOCK_SIZE: usize = 512;

/// Stereo audio processor backed by a VST3 plugin.
///
/// Interleaved `[f32; 2]` frames are deinterleaved into separate L/R buffers
/// for the planar-buffer API expected by VST3, then reinterleaved on output.
pub struct StereoVst3Processor {
    plugin: Vst3Plugin,
    buf_in_l: Vec<f32>,
    buf_in_r: Vec<f32>,
    buf_out_l: Vec<f32>,
    buf_out_r: Vec<f32>,
}

impl StereoVst3Processor {
    /// Create a new stereo VST3 processor from an already-loaded plugin.
    pub fn new(plugin: Vst3Plugin) -> Self {
        Self {
            plugin,
            buf_in_l: vec![0.0f32; BLOCK_SIZE],
            buf_in_r: vec![0.0f32; BLOCK_SIZE],
            buf_out_l: vec![0.0f32; BLOCK_SIZE],
            buf_out_r: vec![0.0f32; BLOCK_SIZE],
        }
    }

    /// Set a normalized parameter value (0.0..=1.0) by plugin parameter ID.
    pub fn set_param(&self, id: u32, normalized: f64) -> anyhow::Result<()> {
        self.plugin.set_param(id, normalized)
    }
}

impl StereoProcessor for StereoVst3Processor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.buf_in_l[0] = input[0];
        self.buf_in_r[0] = input[1];
        self.plugin.process_audio(
            &mut self.buf_in_l[..1],
            &mut self.buf_in_r[..1],
            &mut self.buf_out_l[..1],
            &mut self.buf_out_r[..1],
            1,
        );
        [self.buf_out_l[0], self.buf_out_r[0]]
    }

    fn process_block(&mut self, frames: &mut [[f32; 2]]) {
        let mut offset = 0;
        while offset < frames.len() {
            let chunk = (frames.len() - offset).min(BLOCK_SIZE);

            // Deinterleave: [[L,R], [L,R], ...] → separate L and R vectors.
            for i in 0..chunk {
                self.buf_in_l[i] = frames[offset + i][0];
                self.buf_in_r[i] = frames[offset + i][1];
            }

            self.plugin.process_audio(
                &mut self.buf_in_l[..chunk],
                &mut self.buf_in_r[..chunk],
                &mut self.buf_out_l[..chunk],
                &mut self.buf_out_r[..chunk],
                chunk,
            );

            // Interleave: separate L/R → [[L,R], [L,R], ...].
            for i in 0..chunk {
                frames[offset + i][0] = self.buf_out_l[i];
                frames[offset + i][1] = self.buf_out_r[i];
            }

            offset += chunk;
        }
    }
}
