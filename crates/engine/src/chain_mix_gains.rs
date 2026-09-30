//! Responsibility: holds the fader targets of every chain's own mixer.
//!
//! Issue #1007. The compact chain view gives a chain its own fader on each
//! physical endpoint it plays through, on top of that endpoint's global
//! fader, plus a fader for its DI loop. One lock-free scalar per
//! (chain, direction, device, channels) and one per chain for the DI: the
//! chain id is part of the key, so two chains on the same interface never
//! share a control value. The tables are touched only off the audio thread;
//! the audio thread only loads the atomics its own runtime holds.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use domain::mixer_gain::strip_linear_gain;
use domain::mixer_strip::{MixerDirection, MixerStripId};
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::chain::Chain;

use crate::mixer_gains::EndpointGain;

type ChainEndpointKey = (String, MixerDirection, String, Vec<usize>);

/// Push `chain.mix` into the engine: every endpoint the chain plays through
/// gets its chain fader (unity when nobody moved it), and the DI loop its
/// own. Live runtimes glide to the new targets on their next callback.
pub fn apply_chain_mix(chain: &Chain, registry: &[IoBinding]) {
    // One physical endpoint can reach the chain through several bindings;
    // its fader is the one stored on the FIRST port that reaches it (the
    // port the chain mixer commands address), never reset by a later one.
    let mut written: Vec<MixerStripId> = Vec::new();
    for port in resolve_chain_ports(chain, registry) {
        let direction = match port.direction {
            PortDirection::Input => MixerDirection::Input,
            PortDirection::Output => MixerDirection::Output,
        };
        let strip = MixerStripId {
            direction,
            device_id: port.endpoint.device_id.0.clone(),
            channels: port.endpoint.channels.clone(),
        };
        if written.contains(&strip) {
            continue;
        }
        let linear = chain
            .mix
            .endpoint(direction, &port.binding_id, &port.endpoint.name)
            .map_or(1.0, |fader| strip_linear_gain(fader.gain_db, fader.muted));
        for channels in strip.runtime_channel_groups(port.endpoint.mode) {
            chain_endpoint_gain(&chain.id, direction, &strip.device_id, &channels).store(linear);
        }
        written.push(strip);
    }
    chain_di_gain(&chain.id).store(strip_linear_gain(chain.mix.di_gain_db, false));
}

/// The linear target `chain`'s own fader holds on one endpoint (1.0 when
/// nobody moved it). Read-only view for controllers and tests.
pub fn chain_endpoint_gain_target(
    chain: &ChainId,
    direction: MixerDirection,
    device_id: &str,
    channels: &[usize],
) -> f32 {
    chain_endpoint_gain(chain, direction, device_id, channels).target()
}

/// The linear target of `chain`'s DI-loop fader.
pub fn chain_di_gain_target(chain: &ChainId) -> f32 {
    chain_di_gain(chain).target()
}

/// The chain-local scalar of one physical endpoint of `chain`.
pub(crate) fn chain_endpoint_gain(
    chain: &ChainId,
    direction: MixerDirection,
    device_id: &str,
    channels: &[usize],
) -> Arc<EndpointGain> {
    let key = (
        chain.0.clone(),
        direction,
        device_id.to_string(),
        channels.to_vec(),
    );
    shared(endpoint_table(), key)
}

/// The DI-loop scalar of `chain`.
pub(crate) fn chain_di_gain(chain: &ChainId) -> Arc<EndpointGain> {
    shared(di_table(), chain.0.clone())
}

fn shared<K: std::hash::Hash + Eq>(
    table: &RwLock<HashMap<K, Arc<EndpointGain>>>,
    key: K,
) -> Arc<EndpointGain> {
    if let Some(gain) = table.read().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return Arc::clone(gain);
    }
    let mut table = table.write().unwrap_or_else(|e| e.into_inner());
    Arc::clone(
        table
            .entry(key)
            .or_insert_with(|| Arc::new(EndpointGain::unity())),
    )
}

fn endpoint_table() -> &'static RwLock<HashMap<ChainEndpointKey, Arc<EndpointGain>>> {
    static TABLE: OnceLock<RwLock<HashMap<ChainEndpointKey, Arc<EndpointGain>>>> = OnceLock::new();
    TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn di_table() -> &'static RwLock<HashMap<String, Arc<EndpointGain>>> {
    static TABLE: OnceLock<RwLock<HashMap<String, Arc<EndpointGain>>>> = OnceLock::new();
    TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}
