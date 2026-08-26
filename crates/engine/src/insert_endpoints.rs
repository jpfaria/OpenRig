//! Responsibility: turns an insert block into the endpoints that serve it.

use domain::io_binding::IoBinding;
use project::block::{AudioBlockKind, InsertBlock};
use project::chain::{ChainInputMode, ChainOutputMode};

use crate::endpoint_entry::{InputEntry, OutputEntry};

/// Whether an Insert block is a real send/return boundary: BOTH sides of its
/// binding have to resolve (#881). A half- or un-resolved insert appends no
/// shim on either side, so the segment walker never points at an endpoint that
/// was not created — the chain simply flows through it.
pub(crate) fn insert_is_bound(kind: &AudioBlockKind, registry: &[IoBinding]) -> bool {
    match kind {
        AudioBlockKind::Insert(ib) => {
            insert_send_as_output_entry(ib, registry).is_some()
                && insert_return_as_input_entry(ib, registry).is_some()
        }
        _ => false,
    }
}

/// Resolve an `InsertBlock`'s RETURN (the signal coming back from the external
/// gear) to an input endpoint — model A (#716): an insert references one E/S
/// (`io`), and its return comes from that binding's INPUT. `None` if the
/// binding is absent or has no input endpoint.
pub(crate) fn insert_return_as_input_entry(
    insert: &InsertBlock,
    registry: &[IoBinding],
) -> Option<InputEntry> {
    let binding = registry.iter().find(|b| b.id == insert.io)?;
    let ep = binding.inputs.first()?;
    Some(InputEntry {
        device_id: ep.device_id.clone(),
        mode: ChainInputMode::from(ep.mode),
        channels: ep.channels.clone(),
    })
}

/// Resolve an `InsertBlock`'s SEND (the signal going out to the external gear)
/// to an output endpoint — it comes from the insert binding's OUTPUT. `None` if
/// the binding is absent or has no output endpoint.
pub(crate) fn insert_send_as_output_entry(
    insert: &InsertBlock,
    registry: &[IoBinding],
) -> Option<OutputEntry> {
    let binding = registry.iter().find(|b| b.id == insert.io)?;
    let ep = binding.outputs.first()?;
    Some(OutputEntry {
        device_id: ep.device_id.clone(),
        mode: ChainOutputMode::try_from(ep.mode).unwrap_or(ChainOutputMode::Stereo),
        channels: ep.channels.clone(),
    })
}
