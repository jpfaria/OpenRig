//! Responsibility: records which streams each chain owns.

use std::collections::HashMap;

use domain::ids::ChainId;

/// What one chain owns right now: the device streams that are open for it
/// and the builds still in flight that would open more (#929).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnedStreams {
    /// `stream_generation` the open streams were built at.
    pub generation: u64,
    pub input_streams: usize,
    pub output_streams: usize,
    /// Cold activations the worker is still building — each one opens a
    /// fresh set of streams when it lands.
    pub activations_in_flight: usize,
    /// Live rebuilds the worker is still building — they swap the runtime
    /// behind the open streams, never open new ones.
    pub rebuilds_in_flight: usize,
}

impl OwnedStreams {
    pub fn is_empty(&self) -> bool {
        self.input_streams == 0
            && self.output_streams == 0
            && self.activations_in_flight == 0
            && self.rebuilds_in_flight == 0
    }
}

/// The in-memory index chain → streams. Every door that opens a stream, or
/// schedules a build that will, writes here; switching a chain off reads it
/// to kill EVERYTHING the chain owns — open streams and builds in flight —
/// so nothing lands afterwards and plays a chain the screen shows as off.
#[derive(Debug, Default)]
pub struct ChainStreamRegistry {
    owned: HashMap<ChainId, OwnedStreams>,
}

impl ChainStreamRegistry {
    pub fn owned(&self, chain: &ChainId) -> Option<&OwnedStreams> {
        self.owned.get(chain)
    }

    /// Whether the chain owns anything — streams or a build on its way.
    pub fn owns(&self, chain: &ChainId) -> bool {
        self.owned.get(chain).is_some_and(|o| !o.is_empty())
    }

    /// A stream build finished: these streams replace whatever the chain had.
    pub fn streams_built(
        &mut self,
        chain: &ChainId,
        generation: u64,
        inputs: usize,
        outputs: usize,
    ) {
        let entry = self.owned.entry(chain.clone()).or_default();
        entry.generation = generation;
        entry.input_streams = inputs;
        entry.output_streams = outputs;
    }

    pub fn activation_started(&mut self, chain: &ChainId) {
        self.owned
            .entry(chain.clone())
            .or_default()
            .activations_in_flight += 1;
    }

    pub fn activation_settled(&mut self, chain: &ChainId) {
        if let Some(o) = self.owned.get_mut(chain) {
            o.activations_in_flight = o.activations_in_flight.saturating_sub(1);
        }
    }

    pub fn rebuild_started(&mut self, chain: &ChainId) {
        self.owned
            .entry(chain.clone())
            .or_default()
            .rebuilds_in_flight += 1;
    }

    pub fn rebuild_settled(&mut self, chain: &ChainId) {
        if let Some(o) = self.owned.get_mut(chain) {
            o.rebuilds_in_flight = o.rebuilds_in_flight.saturating_sub(1);
        }
    }

    /// The chain owns nothing any more. Returns what it owned.
    pub fn forget(&mut self, chain: &ChainId) -> Option<OwnedStreams> {
        self.owned.remove(chain)
    }

    pub fn clear(&mut self) {
        self.owned.clear();
    }
}
