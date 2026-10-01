//! Responsibility: exposes the parameter set a block kind carries.

use super::types::AudioBlockKind;
use crate::param::ParameterSet;

/// The knobs of a block: a model's parameters for `Core` and `Nam`, the split
/// and mixer knobs for `Split` (#328). Ports, inserts and selects carry none —
/// a select's knobs live on its options. Exhaustive on purpose: a new kind
/// must decide here whether scenes, write-back and the parameter commands
/// reach it.
pub fn block_params(kind: &AudioBlockKind) -> Option<&ParameterSet> {
    match kind {
        AudioBlockKind::Core(core) => Some(&core.params),
        AudioBlockKind::Nam(nam) => Some(&nam.params),
        AudioBlockKind::Split(split) => Some(&split.params),
        AudioBlockKind::Select(_)
        | AudioBlockKind::Input(_)
        | AudioBlockKind::Output(_)
        | AudioBlockKind::Insert(_) => None,
    }
}

/// Mutable twin of [`block_params`].
pub fn block_params_mut(kind: &mut AudioBlockKind) -> Option<&mut ParameterSet> {
    match kind {
        AudioBlockKind::Core(core) => Some(&mut core.params),
        AudioBlockKind::Nam(nam) => Some(&mut nam.params),
        AudioBlockKind::Split(split) => Some(&mut split.params),
        AudioBlockKind::Select(_)
        | AudioBlockKind::Input(_)
        | AudioBlockKind::Output(_)
        | AudioBlockKind::Insert(_) => None,
    }
}
