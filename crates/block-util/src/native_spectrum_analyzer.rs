//! Legacy `spectrum_analyzer` block — delegates the FFT/binning DSP to
//! [`feature_dsp::spectrum_fft`]. The block keeps the RT-side ring buffer,
//! hop counter and worker-thread plumbing so existing chains continue to
//! work; the actual signal analysis lives in `feature-dsp` for reuse by the
//! top-bar Spectrum window.
//!
//! This block will be removed in the same milestone that introduces the
//! Spectrum window — see issue #320.

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StreamEntry, StreamHandle,
};
use feature_dsp::spectrum_fft::{SpectrumAnalyzer as DspAnalyzer, BAND_LABELS, FFT_SIZE, HOP_SIZE};
use std::sync::Arc;

use crate::registry::UtilModelDefinition;
use crate::UtilBackendKind;

pub const MODEL_ID: &str = "spectrum_analyzer";
pub const DISPLAY_NAME: &str = "Spectrum Analyzer";

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "utility".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![],
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

/// Worker-side: take an FFT-sized buffer, run the spectrum DSP, publish a
/// snapshot of band-level + peak-hold entries via the stream handle.
fn publish_snapshot(analyzer: &mut DspAnalyzer, stream: &StreamHandle, buffer: &[f32]) {
    let snapshot = analyzer.process(buffer);
    let mut entries = Vec::with_capacity(snapshot.levels.len());
    for (i, &level) in snapshot.levels.iter().enumerate() {
        entries.push(StreamEntry {
            key: format!("band_{i}"),
            value: level,
            text: BAND_LABELS[i].to_string(),
            peak: snapshot.peaks[i],
        });
    }
    stream.store(Arc::new(entries));
}

pub struct SpectrumAnalyzer {
    /// Ring buffer always holds the last FFT_SIZE samples.
    ring: Vec<f32>,
    write_pos: usize,
    /// Counts new samples since the last FFT dispatch.
    hop_count: usize,
    /// RT → worker. Wait-free push.
    to_worker: Arc<crossbeam_queue::ArrayQueue<Vec<f32>>>,
    /// Worker → RT. Wait-free pop. Pre-populated with both buffers at boot
    /// so the RT thread always has something to take when it is time to
    /// dispatch a frame.
    from_worker: Arc<crossbeam_queue::ArrayQueue<Vec<f32>>>,
}

impl SpectrumAnalyzer {
    pub fn new(sample_rate: f32, stream: StreamHandle) -> Self {
        // Two pre-allocated buffers cycled between RT and worker threads.
        // No heap allocation happens in `process_sample` — the RT thread
        // pops a buffer from `from_worker`, fills it from the ring, and
        // pushes it to `to_worker`. The worker pops from `to_worker`,
        // runs the FFT, and recycles the buffer back to `from_worker`.
        let to_worker: Arc<crossbeam_queue::ArrayQueue<Vec<f32>>> =
            Arc::new(crossbeam_queue::ArrayQueue::new(2));
        let from_worker: Arc<crossbeam_queue::ArrayQueue<Vec<f32>>> =
            Arc::new(crossbeam_queue::ArrayQueue::new(2));
        from_worker.push(vec![0.0; FFT_SIZE]).ok();
        from_worker.push(vec![0.0; FFT_SIZE]).ok();

        let to_worker_w = Arc::clone(&to_worker);
        let from_worker_w = Arc::clone(&from_worker);
        let mut analyzer = DspAnalyzer::new(sample_rate);

        std::thread::Builder::new()
            .name("spectrum-analyzer".to_string())
            .spawn(move || loop {
                // Block on a buffer arrival without busy-spinning. Only the
                // RT side must remain wait-free; the worker is allowed to
                // sleep.
                let buf = loop {
                    if let Some(b) = to_worker_w.pop() {
                        break b;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                };
                publish_snapshot(&mut analyzer, &stream, &buf);
                // Recycle the buffer back to the RT side. If the queue is
                // full (RT has not consumed both buffers yet), drop — same
                // back-pressure as the old try_send path.
                from_worker_w.push(buf).ok();
            })
            .expect("spawn spectrum worker");

        Self {
            ring: vec![0.0; FFT_SIZE],
            write_pos: 0,
            hop_count: 0,
            to_worker,
            from_worker,
        }
    }
}

impl MonoProcessor for SpectrumAnalyzer {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.ring[self.write_pos] = input;
        self.write_pos = (self.write_pos + 1) % FFT_SIZE;
        self.hop_count += 1;

        if self.hop_count >= HOP_SIZE {
            self.hop_count = 0;
            // Take a recycled buffer from the worker. If none is available
            // (worker has not finished processing the previous hop), drop
            // this frame — same back-pressure as before, wait-free and
            // zero-alloc on the RT side.
            if let Some(mut buf) = self.from_worker.pop() {
                let read_start = self.write_pos;
                for i in 0..FFT_SIZE {
                    buf[i] = self.ring[(read_start + i) % FFT_SIZE];
                }
                let _ = self.to_worker.push(buf);
            }
        }
        input
    }
}

fn build(
    _params: &ParameterSet,
    sample_rate: usize,
    layout: AudioChannelLayout,
) -> Result<(BlockProcessor, Option<StreamHandle>)> {
    match layout {
        AudioChannelLayout::Mono => {
            let stream: StreamHandle = block_core::new_stream_handle();
            let processor = SpectrumAnalyzer::new(sample_rate as f32, Arc::clone(&stream));
            Ok((BlockProcessor::Mono(Box::new(processor)), Some(stream)))
        }
        AudioChannelLayout::Stereo => anyhow::bail!(
            "spectrum_analyzer uses DualMono; engine should never call build with Stereo layout"
        ),
    }
}

pub const MODEL_DEFINITION: UtilModelDefinition = UtilModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: UtilBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
    stream_kind: "spectrum",
};
