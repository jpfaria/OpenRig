//! Responsibility: names the stream outputs the spectrum rows read.

use engine::stream_io_labels::StreamIoLabels;

/// One `CHAIN  ·  <input> IN <ch>  →  <output> OUT <ch>` per output the
/// stream feeds, upper-cased — the interface plus direction plus 1-based
/// channels on each side, as the chain meters name a stream. A stream with no
/// listed output keeps its aggregate output; one with no resolved E/S keeps
/// its 1-based number.
pub fn spectrum_output_labels(
    chain_label: &str,
    io: Option<&StreamIoLabels>,
    stream_index: usize,
) -> Vec<String> {
    let label = |stream: String| format!("{chain_label}  ·  {stream}").to_uppercase();
    let Some(io) = io else {
        return vec![label(format!("STREAM {}", stream_index + 1))];
    };
    let input = endpoint(&io.input, "IN", &io.input_channels);
    if io.outputs.is_empty() {
        let output = endpoint(&io.output, "OUT", &io.output_channels);
        return vec![label(format!("{input}  →  {output}"))];
    }
    io.outputs
        .iter()
        .map(|out| {
            let output = endpoint(&out.name, "OUT", &out.channels);
            label(format!("{input}  →  {output}"))
        })
        .collect()
}

/// The label of one side (`L`/`R`) of an output row.
pub fn spectrum_row_label(output_label: &str, side: &str) -> String {
    format!("{output_label}  ·  {side}")
}

fn endpoint(name: &str, direction: &str, channels: &str) -> String {
    [name, direction, channels]
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "spectrum_row_label_tests.rs"]
mod tests;
