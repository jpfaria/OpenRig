//! Stereo VST3 processor: wraps `Vst3Plugin` and implements `StereoProcessor`.

use crate::host::Vst3Plugin;
use crate::param_channel::Vst3ParamChannel;
use block_core::StereoProcessor;

/// Internal processing block size for VST3 plugins (matches OpenRig default).
const BLOCK_SIZE: usize = 512;

/// Stereo audio processor backed by a VST3 plugin.
///
/// Interleaved `[f32; 2]` frames are deinterleaved into separate L/R buffers
/// for the planar-buffer API expected by VST3, then reinterleaved on output.
pub struct StereoVst3Processor {
    plugin: Vst3Plugin,
    param_rx: Option<Vst3ParamChannel>,
    buf_in_l: Vec<f32>,
    buf_in_r: Vec<f32>,
    buf_out_l: Vec<f32>,
    buf_out_r: Vec<f32>,
}

impl StereoVst3Processor {
    /// Create a new stereo VST3 processor from an already-loaded plugin.
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

    /// Build a `Vst3GuiContext` that shares this instance's controller, dylib,
    /// and — crucially — the SAME param channel this processor drains, so edits
    /// made in the native editor window reach this very audio instance (#251
    /// out-of-process editor). `None` if the processor has no param channel.
    pub fn make_gui_context(&self) -> Option<crate::param_registry::Vst3GuiContext> {
        let param_channel = self.param_rx.clone()?;
        Some(crate::param_registry::Vst3GuiContext {
            param_channel,
            controller: self.plugin.controller_clone(),
            library: self.plugin.library_arc(),
            // Legacy out-of-process editor path (currently unused); the model id
            // is not threaded here. Registered contexts (engine path) carry it.
            model_id: String::new(),
        })
    }

    /// Set a normalized parameter value (0.0..=1.0) by plugin parameter ID.
    pub fn set_param(&self, id: u32, normalized: f64) -> anyhow::Result<()> {
        self.plugin.set_param(id, normalized)
    }

    /// Read a parameter's current normalized value (0.0..=1.0).
    pub fn get_param(&self, id: u32) -> f64 {
        self.plugin.get_param(id)
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

impl StereoProcessor for StereoVst3Processor {
    fn try_in_place_update(
        &mut self,
        params: &block_core::param::ParameterSet,
        _sample_rate: f32,
    ) -> bool {
        // Apply the new params to the LIVE plugin instead of reloading it.
        // Paths are "p{id}", values are 0..100 (%) → VST3 normalized 0.0..=1.0.
        for (path, value) in params.values.iter() {
            let Some(id) = path.strip_prefix('p').and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            let Some(pct) = value.as_f32() else { continue };
            let normalized = (pct / 100.0).clamp(0.0, 1.0) as f64;
            let _ = self.plugin.set_param(id, normalized);
        }
        true
    }

    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let pending = self.drain_pending_params();
        self.buf_in_l[0] = input[0];
        self.buf_in_r[0] = input[1];
        self.plugin.process_audio(
            &mut self.buf_in_l[..1],
            &mut self.buf_in_r[..1],
            &mut self.buf_out_l[..1],
            &mut self.buf_out_r[..1],
            1,
            &pending,
        );
        [self.buf_out_l[0], self.buf_out_r[0]]
    }

    fn process_block(&mut self, frames: &mut [[f32; 2]]) {
        let pending = self.drain_pending_params();
        let mut offset = 0;
        while offset < frames.len() {
            let chunk = (frames.len() - offset).min(BLOCK_SIZE);

            for i in 0..chunk {
                self.buf_in_l[i] = frames[offset + i][0];
                self.buf_in_r[i] = frames[offset + i][1];
            }

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
                frames[offset + i][0] = self.buf_out_l[i];
                frames[offset + i][1] = self.buf_out_r[i];
            }

            offset += chunk;
        }
    }
}
