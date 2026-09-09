//! Responsibility: names the binding on each meter row.

use domain::io_binding::IoBinding;
use engine::stream_io_labels::StreamIoLabels;
use project::chain::Chain;

/// #928: one (input, output) binding-name pair per meter row, in row order —
/// read off the same segment map `project_stream_count` counts the rows from,
/// so a row and its name cannot drift apart.
pub fn project_stream_labels(chain: &Chain, io_bindings: &[IoBinding]) -> Vec<StreamIoLabels> {
    engine::stream_io_labels::chain_stream_io_labels(chain, io_bindings)
}
