//! Responsibility: decides whether a port merely repeats the chain's own binding.

use super::types::{AudioBlock, AudioBlockKind};

/// Whether `block` is a legacy leftover that duplicates the chain's OWN I/O.
///
/// #716: a binding-bound chain takes its head input and tail output from
/// `io_binding_ids`, so an `Input`/`Output` block pointing at one of those same
/// bindings is a duplicate — it opens a second stream on the same device
/// (absurd latency + underruns) and must be dropped.
///
/// #85: a port pointing at ANY OTHER binding is a mid port the user placed on
/// purpose (e.g. an aux send to a second E/S). It is not a duplicate and must
/// survive — both when projecting the rig and when saving back into it. An
/// unbound port (`io` empty) is one the user just added and has not pointed
/// anywhere yet — it survives too, or it would vanish before it could ever be
/// configured.
pub fn duplicates_chain_binding(block: &AudioBlock, io_binding_ids: &[String]) -> bool {
    let io = match &block.kind {
        AudioBlockKind::Input(b) => &b.io,
        AudioBlockKind::Output(b) => &b.io,
        // Not a port: an `Insert` sends and returns on the same chain, so it
        // never duplicates the chain's head/tail I/O; the rest carry no binding.
        AudioBlockKind::Nam(_)
        | AudioBlockKind::Core(_)
        | AudioBlockKind::Select(_)
        | AudioBlockKind::Insert(_) => return false,
    };
    !io.is_empty() && io_binding_ids.iter().any(|id| id == io)
}
