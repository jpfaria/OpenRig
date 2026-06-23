//! Load-time helper that guarantees every chain has at least one
//! `OutputBlock`.
//!
//! Background: rig-backed chains derive their Output blocks from
//! `rig.outputs` + `input.routing` (see `engine::rig_runtime::rig_to_chains`).
//! Projects saved before the `SaveChainOutputEndpoints` rig-persistence
//! fix left `rig.outputs` empty, so reopening such a project produces
//! chains with no Output. `validate_project` then rejects them
//! ("chain '...' has no output blocks"), the runtime refuses to start,
//! the user has no audio AND no way to enable the chain.
//!
//! This helper is the migration safety net: when a chain has no Output
//! block, append a sensible default routed to `default_device`. The user
//! can later customise via the I/O editor; in the meantime they always
//! have sound on open and previously-broken projects self-heal forward.
//!
//! No-op when (a) the chain already has an Output, or (b) the supplied
//! `default_device` is empty (we'd rather surface the validate error
//! than insert a useless silent Output).

use crate::block::{AudioBlock, AudioBlockKind, OutputBlock, OutputEntry};
use crate::chain::ChainOutputMode;
use crate::project::Project;
use domain::ids::{BlockId, DeviceId};

pub fn ensure_chains_have_output(project: &mut Project, default_device: &DeviceId) {
    if default_device.0.trim().is_empty() {
        return;
    }
    for chain in &mut project.chains {
        let has_output = chain
            .blocks
            .iter()
            .any(|b| matches!(b.kind, AudioBlockKind::Output(_)));
        if has_output {
            continue;
        }
        chain.blocks.push(AudioBlock {
            id: BlockId(format!("{}:out", chain.id.0)),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".into(),
                io: String::new(),
                endpoint: String::new(),
                entries: vec![OutputEntry {
                    device_id: default_device.clone(),
                    mode: ChainOutputMode::Stereo,
                    channels: vec![0, 1],
                }],
            }),
        });
    }
}
