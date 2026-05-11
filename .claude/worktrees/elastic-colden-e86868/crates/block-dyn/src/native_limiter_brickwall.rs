//! Brick wall limiter — native Rust implementation.
//!
//! Architecture is split across submodules so each piece stays small and
//! testable in isolation:
//!
//! - `lookahead` — delay line with O(1) sliding-window peak tracking.
//! - `gain` — peak-to-gain curve with soft knee, instant attack, log release.
//! - `params` — user-facing parameter schema and defaults.
//! - `mono` / `stereo` — MonoProcessor and StereoProcessor impls; the stereo
//!   variant links L/R so gain reduction preserves the image.
//!
//! A chain in Mono layout builds `mono::BrickWallLimiterMono`; a Stereo chain
//! builds `stereo::BrickWallLimiterStereo`. The schema advertises
//! `ModelAudioMode::DualMono` so both layouts are accepted by the block
//! framework.
//!
//! True peak / intersample-peak detection (4×/8× oversampling) is intentionally
//! deferred to a follow-up — see the issue tracker.

#[path = "native_limiter_brickwall/gain.rs"]
mod gain;
#[path = "native_limiter_brickwall/lookahead.rs"]
mod lookahead;
#[path = "native_limiter_brickwall/mono.rs"]
mod mono;
#[path = "native_limiter_brickwall/params.rs"]
mod params;
#[path = "native_limiter_brickwall/stereo.rs"]
mod stereo;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::registry::DynModelDefinition;
use crate::DynBackendKind;

fn schema() -> Result<ModelParameterSchema> {
    Ok(params::model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let p = params::params_from_set(params)?;
    Ok(match layout {
        AudioChannelLayout::Mono => {
            BlockProcessor::Mono(Box::new(mono::BrickWallLimiterMono::new(p, sample_rate)))
        }
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(
            stereo::BrickWallLimiterStereo::new(p, sample_rate),
        )),
    })
}

pub const MODEL_DEFINITION: DynModelDefinition = DynModelDefinition {
    id: params::MODEL_ID,
    display_name: params::DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: DynBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
