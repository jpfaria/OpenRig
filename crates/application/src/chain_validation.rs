//! Pure validations for chain operations.
//!
//! The channel-conflict check here is transport-agnostic: it can be called by
//! `adapter-gui`, `adapter-grpc`, `adapter-mcp`, or any future transport
//! without pulling in any UI types.
//!
//! **Spec reference:** `docs/superpowers/specs/2026-04-23-command-dispatch-architecture-design.md`

use domain::ids::ChainId;
use project::block::AudioBlockKind;
use project::chain::Chain;
use project::project::Project;

/// Error produced by `validate_no_channel_conflict`.
#[derive(Debug, Clone, PartialEq)]
pub enum ChannelConflictError {
    /// `target` would fight with the named chain for at least one device channel.
    ChannelConflict { conflicting_chain_name: String },
}

impl std::fmt::Display for ChannelConflictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelConflictError::ChannelConflict {
                conflicting_chain_name,
            } => write!(f, "channel conflict with chain '{conflicting_chain_name}'"),
        }
    }
}

/// Validate that enabling `target` does not collide with any other currently-
/// enabled chain's input channels.
///
/// * `skip_id` — when validating a *toggle* of an existing chain, pass its id
///   so it is excluded from comparison (otherwise a chain would always conflict
///   with itself). When validating the *add* of a brand-new chain, pass `None`.
///
/// # Algorithm
///
/// For every `(device_id, channel)` pair referenced by `target`'s InputBlocks,
/// we check whether any other enabled chain (those with `chain.enabled == true`,
/// excluding `skip_id`) also references that exact pair.  If so, we return
/// `Err(ChannelConflict)` naming the first conflicting chain.
///
/// The check is purely on the data model — no audio engine is queried.
pub fn validate_no_channel_conflict(
    project: &Project,
    target: &Chain,
    skip_id: Option<&ChainId>,
) -> Result<(), ChannelConflictError> {
    // Collect (device_id, channel) pairs for `target`.
    let target_pairs: Vec<(String, usize)> = collect_input_pairs(target);
    if target_pairs.is_empty() {
        // No inputs → nothing to conflict.
        return Ok(());
    }

    for other in &project.chains {
        // Skip the chain we're toggling (it's being validated from its
        // own perspective — we don't want self-conflict).
        if let Some(skip) = skip_id {
            if &other.id == skip {
                continue;
            }
        }
        if !other.enabled {
            continue;
        }
        let other_pairs = collect_input_pairs(other);
        for tp in &target_pairs {
            if other_pairs.contains(tp) {
                let other_name = other
                    .description
                    .clone()
                    .unwrap_or_else(|| other.id.0.clone());
                return Err(ChannelConflictError::ChannelConflict {
                    conflicting_chain_name: other_name,
                });
            }
        }
    }
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Collect all (device_id, channel) pairs from a chain's InputBlocks.
fn collect_input_pairs(chain: &Chain) -> Vec<(String, usize)> {
    let mut pairs = Vec::new();
    for block in &chain.blocks {
        if let AudioBlockKind::Input(input) = &block.kind {
            for entry in &input.entries {
                for &ch in &entry.channels {
                    pairs.push((entry.device_id.0.clone(), ch));
                }
            }
        }
    }
    pairs
}
