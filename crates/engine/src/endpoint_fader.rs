//! Responsibility: gives one stream endpoint the fader gain it plays at.
//!
//! Issue #1007. An endpoint of a chain is scaled by two faders in series:
//! the global mixer's fader of that physical endpoint and the chain's own
//! fader of it. The stream reads the product; both at unity give exactly
//! 1.0, so an untouched pair stays bit-identical.

use std::sync::Arc;

use domain::ids::ChainId;
use domain::mixer_strip::MixerDirection;

use crate::mixer_gains::EndpointGain;

#[derive(Debug, Clone)]
pub(crate) struct EndpointFader {
    global: Arc<EndpointGain>,
    chain: Arc<EndpointGain>,
}

impl EndpointFader {
    /// The faders of `chain`'s physical endpoint (direction, device, channels).
    pub(crate) fn of(
        chain: &ChainId,
        direction: MixerDirection,
        device_id: &str,
        channels: &[usize],
    ) -> Self {
        Self {
            global: crate::mixer_gains::endpoint_gain(direction, device_id, channels),
            chain: crate::chain_mix_gains::chain_endpoint_gain(
                chain, direction, device_id, channels,
            ),
        }
    }

    /// Linear gain the stream should reach; 1.0 when both faders are at unity.
    #[inline]
    pub(crate) fn target(&self) -> f32 {
        self.global.target() * self.chain.target()
    }

    /// Same control values as `other` — a rebuilt route can keep its state.
    pub(crate) fn same_as(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.global, &other.global) && Arc::ptr_eq(&self.chain, &other.chain)
    }
}
