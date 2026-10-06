//! Responsibility: names the audio interface on each meter row.

use domain::io_binding::IoBinding;
use domain::AudioDeviceDescriptor;
use engine::stream_io_labels::StreamIoLabels;
use project::chain::Chain;

/// One label set per meter row, in row order — read off the same segment map
/// `project_stream_count` counts the rows from, so a row and its name cannot
/// drift apart. Each side is named after the interface it touches, as the host
/// names it in `devices`; an interface missing there leaves the name blank and
/// the row shows only its direction and channels.
pub fn project_stream_labels(
    chain: &Chain,
    io_bindings: &[IoBinding],
    devices: &[AudioDeviceDescriptor],
) -> Vec<StreamIoLabels> {
    let name = |id: &str| {
        devices
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.name.clone())
            .unwrap_or_default()
    };
    engine::stream_io_labels::chain_stream_io_labels(chain, io_bindings)
        .into_iter()
        .map(|mut row| {
            row.input = name(&row.input_device);
            let mut outputs: Vec<String> = Vec::new();
            for output in &mut row.outputs {
                output.name = name(&output.device);
                if !output.name.is_empty() && !outputs.contains(&output.name) {
                    outputs.push(output.name.clone());
                }
            }
            row.output = outputs.join(" + ");
            row
        })
        .collect()
}

#[cfg(test)]
#[path = "meter_row_labels_tests.rs"]
mod tests;
