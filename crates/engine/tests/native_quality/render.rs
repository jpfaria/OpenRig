//! Responsibility: runs a native model over a signal the way the engine does.

use anyhow::{anyhow, Result};
use block_core::param::ParameterSet;
use block_core::BlockProcessor;

use super::catalog::NativeModel;

/// Frames per callback, the engine's small-buffer case.
const BLOCK: usize = 64;

/// The model's output for one mono input, both channels.
pub struct Rendered {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl Rendered {
    pub fn channels(&self) -> [&[f32]; 2] {
        [&self.left, &self.right]
    }

    pub fn all_finite(&self) -> bool {
        self.left.iter().chain(&self.right).all(|s| s.is_finite())
    }
}

/// The schema defaults, or the error the model's own schema gives.
pub fn default_params(model: &NativeModel) -> Result<ParameterSet> {
    ParameterSet::default()
        .normalized_against(&model.schema)
        .map_err(|e| anyhow!(e))
}

/// Build `model` with `params` at `sample_rate` and run `input` through it
/// in engine-sized blocks. Mono input is broadcast to both channels, as the
/// engine does for a mono source on a stereo stream.
pub fn render(
    model: &NativeModel,
    params: &ParameterSet,
    sample_rate: f32,
    input: &[f32],
) -> Result<Rendered> {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let processor = model.build(params, sample_rate)?;
        Ok(run(processor, input))
    }));
    outcome.unwrap_or_else(|payload| Err(anyhow!("PANIC: {}", panic_text(&payload))))
}

fn panic_text(payload: &Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".into())
}

/// Run an already-built processor over `input`.
pub fn run(processor: BlockProcessor, input: &[f32]) -> Rendered {
    match processor {
        BlockProcessor::Mono(mut p) => {
            let mut out = input.to_vec();
            for chunk in out.chunks_mut(BLOCK) {
                p.process_block(chunk);
            }
            Rendered {
                left: out.clone(),
                right: out,
            }
        }
        BlockProcessor::Stereo(mut p) => {
            let mut frames: Vec<[f32; 2]> = input.iter().map(|&s| [s, s]).collect();
            for chunk in frames.chunks_mut(BLOCK) {
                p.process_block(chunk);
            }
            Rendered {
                left: frames.iter().map(|f| f[0]).collect(),
                right: frames.iter().map(|f| f[1]).collect(),
            }
        }
    }
}
