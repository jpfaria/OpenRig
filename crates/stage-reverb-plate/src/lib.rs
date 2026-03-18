//! Plate reverb implementations.
pub mod foundation;

use anyhow::{bail, Result};
use foundation::FoundationPlateReverb;
use stage_core::{MonoProcessor, NamedModel};

pub const DEFAULT_REVERB_MODEL: &str = "plate_foundation";

pub enum ReverbModel {
    PlateFoundation,
}

impl NamedModel for ReverbModel {
    fn model_key(&self) -> &'static str {
        match self {
            ReverbModel::PlateFoundation => DEFAULT_REVERB_MODEL,
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            ReverbModel::PlateFoundation => "Plate Foundation Reverb",
        }
    }
}

pub struct ReverbParams {
    pub room_size: f32,
    pub damping: f32,
    pub mix: f32,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            room_size: 0.45,
            damping: 0.35,
            mix: 0.25,
        }
    }
}

pub fn build_reverb_processor(
    model: &str,
    params: ReverbParams,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    match model {
        DEFAULT_REVERB_MODEL | "plate" | "spring" | "hall" | "room" => {
            Ok(Box::new(FoundationPlateReverb::new(params, sample_rate)))
        }
        other => bail!("unsupported reverb model '{}'", other),
    }
}
