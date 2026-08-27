//! Responsibility: finds two chains fighting over the same input channel.

use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use project::chain::Chain;
use project::project::Project;

use crate::endpoint_entry::{resolve_chain_io, InputEntry};

/// #716: the first `(device_id, channel)` claimed by two different input
/// endpoints in `inputs`, or `None` when every input tap is unique.
///
/// Two or more ACTIVE inputs may not read the same physical capture point at
/// once (invariant #4). The activation path feeds this the resolved input
/// endpoints of all enabled chains (head/tail + mid) — within a chain AND
/// across chains — and refuses to bring up a chain that would collide. Same
/// device on DIFFERENT channels is fine (the "two E/S on one device" case);
/// outputs are never checked (many inputs may feed one output).
pub fn input_port_conflict(inputs: &[InputEntry]) -> Option<(String, usize)> {
    let mut claimed: std::collections::HashSet<(String, usize)> = std::collections::HashSet::new();
    input_taps(inputs)
        .into_iter()
        .find(|tap| !claimed.insert(tap.clone()))
}

/// Every physical capture point `(device_id, channel)` these input endpoints
/// read, in declaration order.
fn input_taps(inputs: &[InputEntry]) -> Vec<(String, usize)> {
    inputs
        .iter()
        .flat_map(|entry| {
            entry
                .channels
                .iter()
                .map(|&ch| (entry.device_id.0.clone(), ch))
        })
        .collect()
}

/// An enabled chain already reading a capture point another chain wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputChannelConflict {
    /// The enabled chain that already reads the capture point.
    pub chain: ChainId,
    /// Device id of the contended capture point.
    pub device: String,
    /// Zero-based channel index of the contended capture point.
    pub channel: usize,
}

/// #833: the first capture point `candidate` would steal from an ENABLED chain
/// in `chains`, or `None` when their inputs are disjoint.
///
/// Same resolution the activation path uses ([`input_conflicting_chains`]), but
/// it names the contended tap and its holder so a command handler can REFUSE
/// the change with an explanatory error instead of the runtime silently
/// skipping the chain. The candidate is matched out by id, so passing the
/// project's whole chain list (including the candidate) is safe.
pub fn conflicting_input_channel<'a>(
    candidate: &Chain,
    chains: impl IntoIterator<Item = &'a Chain>,
    registry: &[IoBinding],
) -> Option<InputChannelConflict> {
    let wanted = input_taps(&resolve_chain_io(candidate, registry).0);
    if wanted.is_empty() {
        return None;
    }
    chains
        .into_iter()
        .filter(|other| other.enabled && other.id != candidate.id)
        .find_map(|other| {
            input_taps(&resolve_chain_io(other, registry).0)
                .into_iter()
                .find(|tap| wanted.contains(tap))
                .map(|(device, channel)| InputChannelConflict {
                    chain: other.id.clone(),
                    device,
                    channel,
                })
        })
}

/// #833 load-time normalization: flip off every enabled chain the activation
/// path would skip, so the project state matches what the runtime will do.
/// Returns the ids that were disabled, in project order.
///
/// A project saved before the command-level guard existed (or a hand-edited
/// YAML) can carry two enabled chains on one capture point. The runtime already
/// refuses to bring up the second one — leaving it "enabled" in the project
/// just lies to the user. First chain in project order keeps the channel.
pub fn disable_conflicting_chains(project: &mut Project, registry: &[IoBinding]) -> Vec<ChainId> {
    let skipped = input_conflicting_chains(project.chains.iter(), registry);
    for chain in project
        .chains
        .iter_mut()
        .filter(|c| skipped.contains(&c.id))
    {
        chain.enabled = false;
    }
    skipped
}

/// The chains that must NOT be activated because an earlier (higher-priority,
/// i.e. earlier in iteration) enabled chain already claimed one of their input
/// taps `(device, channel)`. First chain wins; the conflicting ones are
/// returned (skip them at activation). Disabled chains are ignored. Output taps
/// are never considered (many inputs may feed one output). #716, invariant #4.
pub fn input_conflicting_chains<'a>(
    chains: impl IntoIterator<Item = &'a Chain>,
    registry: &[IoBinding],
) -> Vec<ChainId> {
    let mut claimed: Vec<InputEntry> = Vec::new();
    let mut skipped = Vec::new();
    for chain in chains {
        if !chain.enabled {
            continue;
        }
        let (inputs, _) = resolve_chain_io(chain, registry);
        let mut combined = claimed.clone();
        combined.extend(inputs.iter().cloned());
        if input_port_conflict(&combined).is_some() {
            skipped.push(chain.id.clone());
        } else {
            claimed.extend(inputs);
        }
    }
    skipped
}
