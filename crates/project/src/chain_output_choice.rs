//! Responsibility: picks the row of the project's output list a chain's saved output reference points at.

use domain::io_binding::IoBinding;

use crate::binding_discovery::{resolve_chain_ports, PortDirection};
use crate::chain::Chain;
use crate::project_outputs::{output_position, ProjectOutput};

/// Row the DI / looper output picker highlights. A saved `(binding id,
/// endpoint name)` wins while it still names an output of the project, even
/// one outside the chain. With nothing saved, or a stale reference, the chain's
/// main output is what plays, so that row is shown. `-1` when nothing would
/// play: no valid reference and a chain with no output.
pub fn chain_output_index(
    chain: &Chain,
    registry: &[IoBinding],
    outputs: &[ProjectOutput],
    saved: Option<(&str, &str)>,
) -> i32 {
    if let Some(index) =
        saved.and_then(|(binding, endpoint)| output_position(outputs, binding, endpoint))
    {
        return index as i32;
    }
    resolve_chain_ports(chain, registry)
        .into_iter()
        .find(|p| p.direction == PortDirection::Output)
        .and_then(|p| output_position(outputs, &p.binding_id, &p.endpoint.name))
        .map_or(-1, |index| index as i32)
}

#[cfg(test)]
#[path = "chain_output_choice_tests.rs"]
mod tests;
