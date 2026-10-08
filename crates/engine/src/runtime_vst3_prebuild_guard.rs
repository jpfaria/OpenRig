//! Responsibility: decides whether a live edit may build a fresh VST3 while the runtime keeps processing.
//!
//! #1081: an edit that needs a fresh VST3 (one switched on for the first time,
//! a model swap) used to take the quiesced path every time: every node left
//! the pipeline and the chain played its raw input for the whole plugin
//! build — the gap and the click when a reverb is switched on. Creating an
//! instance is only unsafe while an instance of the SAME bundle is inside
//! `process()` (#779: its JUCE state is shared within the bundle). So a fresh
//! VST3 is built ahead, with the pipeline playing, unless this runtime holds a
//! live instance of its bundle, or a node that may hide one.

use project::block::AudioBlock;

use crate::runtime_state::{BlockRuntimeNode, RuntimeProcessor};

/// What a live node holds that a fresh VST3 instance could collide with.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LiveInstance {
    /// A VST3 instance of this bundle.
    Vst3(String),
    /// A node whose inner blocks the edit's walk does not see (a `Select`, a
    /// `Split`).
    Opaque,
    /// Nothing a VST3 build can collide with.
    Unrelated,
}

impl LiveInstance {
    pub(crate) fn of(node: &BlockRuntimeNode) -> Self {
        match &node.processor {
            RuntimeProcessor::Select(_) | RuntimeProcessor::Split(_) => LiveInstance::Opaque,
            RuntimeProcessor::Audio(_) if is_vst3(&node.block_snapshot) => {
                vst3_bundle(&node.block_snapshot).map_or(LiveInstance::Opaque, |bundle| {
                    LiveInstance::Vst3(bundle.into())
                })
            }
            RuntimeProcessor::Audio(_) | RuntimeProcessor::Bypass => LiveInstance::Unrelated,
        }
    }
}

/// The bundle of a VST3 block: `vst3:<bundle>:<class>`.
pub(crate) fn vst3_bundle(block: &AudioBlock) -> Option<&str> {
    let model = block
        .model_ref()
        .filter(|model| model.effect_type == block_core::EFFECT_TYPE_VST3)?
        .model;
    let mut parts = model.split(':');
    let (Some("vst3"), Some(bundle), Some(class)) = (parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    (!bundle.is_empty() && !class.is_empty()).then_some(bundle)
}

/// `true` when `block`, a VST3 the edit needs fresh, can be built while the
/// `live` nodes keep processing.
pub(crate) fn fresh_vst3_may_prebuild(block: &AudioBlock, live: &[LiveInstance]) -> bool {
    let Some(bundle) = vst3_bundle(block) else {
        return false;
    };
    !live.iter().any(|instance| match instance {
        LiveInstance::Opaque => true,
        LiveInstance::Vst3(live_bundle) => live_bundle == bundle,
        LiveInstance::Unrelated => false,
    })
}

fn is_vst3(block: &AudioBlock) -> bool {
    block
        .model_ref()
        .is_some_and(|model| model.effect_type == block_core::EFFECT_TYPE_VST3)
}

#[cfg(test)]
#[path = "runtime_vst3_prebuild_guard_tests.rs"]
mod tests;
