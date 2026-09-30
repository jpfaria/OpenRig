//! Responsibility: holds the global-mixer fader target of every physical endpoint.
//!
//! Issue #1007. One lock-free scalar per physical endpoint (direction,
//! device, channels). The table is touched only off the audio thread: the
//! control plane writes a target, a graph build hands the route / input
//! state an `Arc` to its endpoint's scalar. The audio thread only loads
//! that atomic — each stream reads its own endpoint independently, nothing
//! is summed or shared beyond the control value itself.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use domain::mixer_strip::MixerDirection;

/// The fader target of one physical endpoint, as linear gain bits.
#[derive(Debug)]
pub struct EndpointGain {
    target_bits: AtomicU32,
}

impl EndpointGain {
    pub(crate) fn unity() -> Self {
        Self {
            target_bits: AtomicU32::new(1.0_f32.to_bits()),
        }
    }

    /// Linear target; 1.0 when nobody touched the fader.
    #[inline]
    pub fn target(&self) -> f32 {
        f32::from_bits(self.target_bits.load(Ordering::Relaxed))
    }

    /// Set the linear target; live streams glide to it on their next callback.
    pub(crate) fn store(&self, linear: f32) {
        self.target_bits
            .store(linear.max(0.0).to_bits(), Ordering::Relaxed);
    }
}

/// Set the linear fader target of one physical endpoint. Live runtimes
/// already holding that endpoint glide to it on their next callback.
pub fn set_endpoint_gain(
    direction: MixerDirection,
    device_id: &str,
    channels: &[usize],
    linear: f32,
) {
    endpoint_gain(direction, device_id, channels).store(linear);
}

/// The current linear target of one physical endpoint.
pub fn endpoint_gain_target(direction: MixerDirection, device_id: &str, channels: &[usize]) -> f32 {
    endpoint_gain(direction, device_id, channels).target()
}

/// The shared scalar a route / input state of that endpoint reads.
pub(crate) fn endpoint_gain(
    direction: MixerDirection,
    device_id: &str,
    channels: &[usize],
) -> Arc<EndpointGain> {
    let key = (direction, device_id.to_string(), channels.to_vec());
    if let Some(gain) = table().read().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return Arc::clone(gain);
    }
    let mut table = table().write().unwrap_or_else(|e| e.into_inner());
    Arc::clone(
        table
            .entry(key)
            .or_insert_with(|| Arc::new(EndpointGain::unity())),
    )
}

type EndpointKey = (MixerDirection, String, Vec<usize>);

fn table() -> &'static RwLock<HashMap<EndpointKey, Arc<EndpointGain>>> {
    static TABLE: OnceLock<RwLock<HashMap<EndpointKey, Arc<EndpointGain>>>> = OnceLock::new();
    TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}
