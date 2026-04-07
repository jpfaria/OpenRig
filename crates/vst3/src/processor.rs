//! Mono VST3 processor: wraps `Vst3Plugin` and implements `MonoProcessor`.
//!
//! VST3 plugins are inherently stereo (or multi-channel). The mono wrapper
//! feeds the same input signal to both channels and takes the left channel
//! output as the mono result.

use crate::host::Vst3Plugin;
use crate::param_channel::Vst3ParamChannel;
use block_core::MonoProcessor;

/// Internal processing block size for VST3 plugins (matches OpenRig default).
const BLOCK_SIZE: usize = 512;

/// Mono audio processor backed by a VST3 plugin.
///
/// The plugin receives the mono signal on both L and R inputs; the left channel
/// output is used as the mono result. This avoids completely muting plugins
/// that only process one channel.
pub struct Vst3Processor {
    plugin: Vst3Plugin,
    param_rx: Option<Vst3ParamChannel>,
    buf_in_l: Vec<f32>,
    buf_in_r: Vec<f32>,
    buf_out_l: Vec<f32>,
    buf_out_r: Vec<f32>,
}

impl Vst3Processor {
    /// Create a new mono VST3 processor from an already-loaded plugin.
    ///
    /// `param_rx` — optional channel for parameter updates pushed by the
    /// plugin's native GUI via `IComponentHandler::performEdit`.
    pub fn new(plugin: Vst3Plugin, param_rx: Option<Vst3ParamChannel>) -> Self {
        Self {
            plugin,
            param_rx,
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

    /// Apply any pending parameter updates from the GUI before processing.
    fn apply_pending_params(&self) {
        if let Some(rx) = &self.param_rx {
            while let Some(update) = rx.pop() {
                let _ = self.plugin.set_param(update.id, update.normalized);
            }
        }
    }
}

impl MonoProcessor for Vst3Processor {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.apply_pending_params();
        self.buf_in_l[0] = input;
        self.buf_in_r[0] = input;
        self.plugin.process_audio(
            &mut self.buf_in_l[..1],
            &mut self.buf_in_r[..1],
            &mut self.buf_out_l[..1],
            &mut self.buf_out_r[..1],
            1,
        );
        self.buf_out_l[0]
    }

    fn process_block(&mut self, samples: &mut [f32]) {
        self.apply_pending_params();
        let mut offset = 0;
        while offset < samples.len() {
            let chunk = (samples.len() - offset).min(BLOCK_SIZE);

            for i in 0..chunk {
                let v = samples[offset + i];
                self.buf_in_l[i] = v;
                self.buf_in_r[i] = v;
            }

            self.plugin.process_audio(
                &mut self.buf_in_l[..chunk],
                &mut self.buf_in_r[..chunk],
                &mut self.buf_out_l[..chunk],
                &mut self.buf_out_r[..chunk],
                chunk,
            );

            for i in 0..chunk {
                samples[offset + i] = self.buf_out_l[i];
            }

            offset += chunk;
        }
    }
}
