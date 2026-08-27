//! Responsibility: applies a plugin's manifest output gain after its processing.

use crate::dispatch::BlockProcessor;
use crate::processor::{MonoProcessor, StereoProcessor};

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
