//! Responsibility: lists every native model the build registers.

use std::sync::Once;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};
use plugin_loader::manifest::{Backend, BlockType};
use plugin_loader::native_runtimes::{self, NativeRuntime};

type FamilyBuild = fn(&str, &ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>;
type FamilySchema = fn(&str) -> Result<ModelParameterSchema>;

/// How the engine builds a model: through its family crate when the family
/// has one (the family registry resolves the id first, so two families may
/// reuse an id), else through the native runtime table.
enum Builder {
    Family(FamilyBuild),
    Runtime(NativeRuntime),
}

/// One compiled-in model, with what the battery needs to build and drive it.
pub struct NativeModel {
    pub id: String,
    pub block_type: BlockType,
    pub schema: ModelParameterSchema,
    builder: Builder,
}

impl NativeModel {
    /// Saturating families: their job is to add harmonics, so THD and
    /// level are not held to the linear bar and aliasing is measured.
    pub fn is_nonlinear(&self) -> bool {
        matches!(
            self.block_type,
            BlockType::GainPedal | BlockType::Preamp | BlockType::Amp
        )
    }

    /// Families whose output is meant to differ from a steady sine
    /// (sidebands, pitch change, envelope following), so THD+N on a sine
    /// is not a quality measure for them.
    pub fn is_time_varying(&self) -> bool {
        matches!(
            self.block_type,
            BlockType::Mod | BlockType::Wah | BlockType::Pitch | BlockType::Dyn
        )
    }

    /// The layout the engine builds this model with for a mono source
    /// (`runtime_processor_model`): one mono instance for mono-only and
    /// dual-mono models, a stereo one otherwise.
    pub fn layout(&self) -> AudioChannelLayout {
        match self.schema.audio_mode {
            ModelAudioMode::MonoOnly | ModelAudioMode::DualMono => AudioChannelLayout::Mono,
            ModelAudioMode::TrueStereo | ModelAudioMode::MonoToStereo => AudioChannelLayout::Stereo,
        }
    }

    pub fn build(&self, params: &ParameterSet, sample_rate: f32) -> Result<BlockProcessor> {
        match &self.builder {
            Builder::Family(build) => build(&self.id, params, sample_rate, self.layout()),
            Builder::Runtime(runtime) => (runtime.build)(params, sample_rate, self.layout()),
        }
    }
}

fn family(block_type: BlockType) -> Option<(FamilySchema, FamilyBuild)> {
    Some(match block_type {
        BlockType::Amp => (
            block_amp::amp_model_schema,
            block_amp::build_amp_processor_for_layout,
        ),
        BlockType::Cab => (
            block_cab::cab_model_schema,
            block_cab::build_cab_processor_for_layout,
        ),
        BlockType::Delay => (
            block_delay::delay_model_schema,
            block_delay::build_delay_processor_for_layout,
        ),
        BlockType::Filter => (
            block_filter::filter_model_schema,
            block_filter::build_filter_processor_for_layout,
        ),
        BlockType::GainPedal => (
            block_gain::gain_model_schema,
            block_gain::build_gain_processor_for_layout,
        ),
        BlockType::Mod => (
            block_mod::modulation_model_schema,
            block_mod::build_modulation_processor_for_layout,
        ),
        BlockType::Pitch => (
            block_pitch::pitch_model_schema,
            block_pitch::build_pitch_processor_for_layout,
        ),
        BlockType::Preamp => (
            block_preamp::preamp_model_schema,
            block_preamp::build_preamp_processor_for_layout,
        ),
        BlockType::Reverb => (
            block_reverb::reverb_model_schema,
            block_reverb::build_reverb_processor_for_layout,
        ),
        BlockType::Wah => (
            block_wah::wah_model_schema,
            block_wah::build_wah_processor_for_layout,
        ),
        _ => return None,
    })
}

/// Every native model, in registration order.
pub fn native_models() -> Vec<NativeModel> {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        engine::native_registry::register_all_natives();
        plugin_loader::registry::init_many(&[]);
    });
    plugin_loader::registry::packages()
        .iter()
        .filter_map(|package| {
            let Backend::Native { runtime_id } = &package.manifest.backend else {
                return None;
            };
            let id = package.manifest.id.clone();
            let block_type = package.manifest.block_type;
            let (schema, builder) = match family(block_type) {
                Some((schema, build)) => (schema(&id).ok()?, Builder::Family(build)),
                None => {
                    let runtime = native_runtimes::get(runtime_id)?;
                    ((runtime.schema)().ok()?, Builder::Runtime(runtime))
                }
            };
            Some(NativeModel {
                id,
                block_type,
                schema,
                builder,
            })
        })
        .collect()
}
