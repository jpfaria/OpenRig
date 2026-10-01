//! Responsibility: builds runtime nodes with a known delay for split runtime tests.
//!
//! Test fixtures for the split runtime (#328): hand-built runtime nodes whose
//! processors delay or scale by a known amount.

use block_core::{AudioChannelLayout, MonoProcessor};
use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::param::ParameterSet;

use crate::runtime_audio_frame::AudioProcessor;
use crate::runtime_block_builders::audio_block_runtime_node;
use crate::runtime_state::{BlockRuntimeNode, FadeState, ProcessorBuildOutcome};

/// Delays its input by exactly `samples` samples — and says so.
pub(crate) struct FixedDelay {
    ring: Vec<f32>,
    pos: usize,
}

impl FixedDelay {
    pub(crate) fn new(samples: usize) -> Self {
        Self {
            ring: vec![0.0; samples],
            pos: 0,
        }
    }
}

impl MonoProcessor for FixedDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        if self.ring.is_empty() {
            return input;
        }
        let out = self.ring[self.pos];
        self.ring[self.pos] = input;
        self.pos = (self.pos + 1) % self.ring.len();
        out
    }

    fn latency_samples(&self) -> usize {
        self.ring.len()
    }
}

/// Multiplies by a fixed gain, with no delay.
pub(crate) struct Scale(pub(crate) f32);

impl MonoProcessor for Scale {
    fn process_sample(&mut self, input: f32) -> f32 {
        input * self.0
    }
}

fn snapshot(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

/// An enabled, fully faded-in node running `processor`.
pub(crate) fn mono_node(id: &str, processor: Box<dyn MonoProcessor>) -> BlockRuntimeNode {
    let mut node = audio_block_runtime_node(
        &snapshot(id),
        AudioChannelLayout::Stereo,
        true,
        ProcessorBuildOutcome {
            processor: AudioProcessor::Mono(processor),
            output_layout: AudioChannelLayout::Stereo,
            stream_handle: None,
        },
    );
    node.fade_state = FadeState::Active;
    node
}

pub(crate) fn delay_node(id: &str, samples: usize) -> BlockRuntimeNode {
    mono_node(id, Box::new(FixedDelay::new(samples)))
}

pub(crate) fn gain_node(id: &str, gain: f32) -> BlockRuntimeNode {
    mono_node(id, Box::new(Scale(gain)))
}
