//! Responsibility: detects when a chain's meters must be resubscribed.

use application::audio_taps::AudioTaps;

/// Walk the project's chains, compute each one's current
/// `timer_chain_signature`, compare against the cached value, and
/// return the list of chains whose signature changed (must be
/// re-subscribed).
///
/// Nothing hosted ([`AudioTaps::is_hosted`] false) means the runtime is
/// offline. The whole cache is wiped so the next online tick treats
/// every chain as a fresh subscription — without this, toggling off the
/// last enabled chain drops the runtime, and the subsequent toggle-on
/// (which spins up a NEW one with the same project state) would produce
/// the same cached signature, skip invalidation, and leave the meter
/// store handing out taps opened against the dropped runtime.
pub fn detect_invalidations(
    chains: &[project::chain::Chain],
    taps: &dyn AudioTaps,
    last_signature: &mut std::collections::HashMap<domain::ids::ChainId, u64>,
) -> Vec<domain::ids::ChainId> {
    let chain_ids: Vec<_> = chains.iter().map(|c| c.id.clone()).collect();
    if !taps.is_hosted() {
        last_signature.clear();
        return Vec::new();
    }
    let mut invalidate = Vec::new();
    for c in chains.iter() {
        let sig = timer_chain_signature(c, taps.stream_count(&c.id));
        if last_signature.get(&c.id).copied() != Some(sig) {
            invalidate.push(c.id.clone());
            last_signature.insert(c.id.clone(), sig);
        }
    }
    last_signature.retain(|cid, _| chain_ids.contains(cid));
    invalidate
}

/// Full per-tick "did anything that requires a re-subscribe change?"
/// signature: project-side bits AND the engine's current stream
/// count for this chain. Stream count is the SUM across this chain's
/// per-input runtimes (issue #350) and drops to 0 when the engine
/// tears them down (chain toggle off, rig-nav rebuild, device
/// reopen). Folding it into the signature is what makes the timer
/// invalidate the dead ring handles a teardown leaves behind —
/// `chain.enabled` alone is not enough because the project state and
/// the engine state can disagree during the rebuild window. Hashes
/// `(chain_meter_signature, stream_count)` together so neither
/// dimension can mask a change in the other.
pub fn timer_chain_signature(chain: &project::chain::Chain, stream_count: usize) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    chain_meter_signature(chain).hash(&mut h);
    stream_count.hash(&mut h);
    h.finish()
}

/// Compact "did the runtime layout change?" signature for a chain.
/// Includes the chain's enabled flag and every block's `(id, enabled)`
/// — the bits that flip when the runtime is torn down and rebuilt
/// (toggle, rig-nav preset/scene switch, block add/remove). NOT
/// affected by knob/param value changes, so steady-state ticks don't
/// cause a re-subscribe (that's the flicker fix). The meter timer
/// compares the signature against the previous tick's value and
/// invalidates the chain's meter store entry on any difference.
pub fn chain_meter_signature(chain: &project::chain::Chain) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    chain.enabled.hash(&mut h);
    for b in &chain.blocks {
        b.id.0.hash(&mut h);
        b.enabled.hash(&mut h);
    }
    h.finish()
}

/// #85: how many meter rows the chain draws — one per STREAM, i.e. per
/// (input × output) pipeline, which is also how the engine indexes its
/// per-stream taps. A mid `Input`/`Output` is a stream of its own, so it gets
/// its own INPUT/OUTPUT bar; without this the row count came from the resolved
/// INPUTS and a mid port had no meter at all.
pub fn project_stream_count(
    chain: &project::chain::Chain,
    io_bindings: &[domain::io_binding::IoBinding],
) -> usize {
    engine::runtime_graph::chain_stream_count(chain, io_bindings)
}
