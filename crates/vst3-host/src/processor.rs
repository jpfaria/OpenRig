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

    /// Drain pending GUI parameter updates, returning them as a Vec.
    fn drain_pending_params(&self) -> Vec<(u32, f64)> {
        if let Some(rx) = &self.param_rx {
            let mut out = Vec::new();
            while let Some(update) = rx.pop() {
                out.push((update.id, update.normalized));
            }
            out
        } else {
            Vec::new()
        }
    }
}

impl MonoProcessor for Vst3Processor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let pending = self.drain_pending_params();
        self.buf_in_l[0] = input;
        self.buf_in_r[0] = input;
        self.plugin.process_audio(
            &mut self.buf_in_l[..1],
            &mut self.buf_in_r[..1],
            &mut self.buf_out_l[..1],
            &mut self.buf_out_r[..1],
            1,
            &pending,
        );
        self.buf_out_l[0]
    }

    fn process_block(&mut self, samples: &mut [f32]) {
        let pending = self.drain_pending_params();
        let mut offset = 0;
        while offset < samples.len() {
            let chunk = (samples.len() - offset).min(BLOCK_SIZE);

            for i in 0..chunk {
                let v = samples[offset + i];
                self.buf_in_l[i] = v;
                self.buf_in_r[i] = v;
            }

            // Pass pending params only on the first chunk; subsequent chunks
            // get an empty slice (changes already applied by the plugin).
            let params_for_chunk: &[(u32, f64)] = if offset == 0 { &pending } else { &[] };

            self.plugin.process_audio(
                &mut self.buf_in_l[..chunk],
                &mut self.buf_in_r[..chunk],
                &mut self.buf_out_l[..chunk],
                &mut self.buf_out_r[..chunk],
                chunk,
                params_for_chunk,
            );

            for i in 0..chunk {
                samples[offset + i] = self.buf_out_l[i];
            }

            offset += chunk;
        }
    }
}
