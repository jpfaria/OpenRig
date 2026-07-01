//! Per-block processor traits + the dispatch enum used by the engine.
//!
//! Lifted out of `lib.rs` (Phase 6 of issue #194). One responsibility:
//! the contract every block processor implements.

pub trait MonoProcessor: Send + Sync + 'static {
    fn process_sample(&mut self, input: f32) -> f32;
    fn process_block(&mut self, buffer: &mut [f32]) {
        for sample in buffer {
            *sample = self.process_sample(*sample);
        }
    }

    /// Attempt to retune this processor against a new `ParameterSet` without
    /// dropping its internal state. Default returns `false` — caller must do a
    /// full rebuild (the processor cannot adapt without a fresh build).
    ///
    /// Implementations that DO support live retuning (e.g. EQs whose only state
    /// is the IIR sample-history of biquads) override this to mutate coefficients
    /// in place and return `true`. The runtime then keeps the processor — and
    /// crucially its sample history — alive across the parameter change, which
    /// suppresses the click users heard when sliders moved (issue #358).
    ///
    /// Called on the rebuild thread holding exclusive ownership of `self`.
    fn try_in_place_update(
        &mut self,
        _params: &crate::param::ParameterSet,
        _sample_rate: f32,
    ) -> bool {
        false
    }
}

pub trait StereoProcessor: Send + Sync + 'static {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2];
    fn process_block(&mut self, buffer: &mut [[f32; 2]]) {
        for frame in buffer {
            *frame = self.process_frame(*frame);
        }
    }
}

pub enum BlockProcessor {
    Mono(Box<dyn MonoProcessor>),
    Stereo(Box<dyn StereoProcessor>),
}

/// Wraps a [`MonoProcessor`] with a static linear gain applied post-process.
/// Usado pelos `from_package` (LV2 / IR) pra aplicar `manifest.output_gain_db`
/// como baseline objetivo do plugin (issue #491).
struct GainScaledMono {
    inner: Box<dyn MonoProcessor>,
    gain: f32,
}

impl MonoProcessor for GainScaledMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.inner.process_sample(input) * self.gain
    }

    fn process_block(&mut self, buffer: &mut [f32]) {
        self.inner.process_block(buffer);
        for sample in buffer {
            *sample *= self.gain;
        }
    }

    fn try_in_place_update(
        &mut self,
        params: &crate::param::ParameterSet,
        sample_rate: f32,
    ) -> bool {
        self.inner.try_in_place_update(params, sample_rate)
    }
}

/// Wraps a [`StereoProcessor`] with a static linear gain applied post-process.
struct GainScaledStereo {
    inner: Box<dyn StereoProcessor>,
    gain: f32,
}

impl StereoProcessor for GainScaledStereo {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let [l, r] = self.inner.process_frame(input);
        [l * self.gain, r * self.gain]
    }

    fn process_block(&mut self, buffer: &mut [[f32; 2]]) {
        self.inner.process_block(buffer);
        for frame in buffer {
            frame[0] *= self.gain;
            frame[1] *= self.gain;
        }
    }
}

/// Aplica `manifest.output_gain_db` (offset aditivo em dB, 0 = unity) como
/// linha de gain estática pós-process. No-op se `db` é `None` ou `0.0`.
/// Conversão dB→linear: `10^(db/20)`.
///
/// Usado pelos `from_package` dos backends LV2 / IR. NAM aplica via
/// `plugin_params.output_level_db` (NAM C++ host nativo já tem level shift
/// embutido), então não passa por aqui.
pub fn wrap_with_output_gain_db(processor: BlockProcessor, db: Option<f32>) -> BlockProcessor {
    let db = match db {
        Some(d) if d.abs() > f32::EPSILON => d,
        _ => return processor,
    };
    let gain = 10.0_f32.powf(db / 20.0);
    match processor {
        BlockProcessor::Mono(inner) => {
            BlockProcessor::Mono(Box::new(GainScaledMono { inner, gain }))
        }
        BlockProcessor::Stereo(inner) => {
            BlockProcessor::Stereo(Box::new(GainScaledStereo { inner, gain }))
        }
    }
}

pub trait NamedModel {
    fn model_key(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
}

/// Opaque handle to an open plugin editor window.
///
/// Dropping the handle closes the window and releases all resources.
/// The concrete type is an implementation detail of the plugin host crate.
pub trait PluginEditorHandle: Send {
    /// Bring the already-open editor window back to the front.
    ///
    /// Called when the user re-opens an editor that is still held open, so the
    /// host reuses the existing plugin instance instead of creating a new one
    /// (some plugins break their module after a window close + reload).
    fn focus(&self) {}
}

#[cfg(test)]
#[path = "traits_tests.rs"]
mod tests;
