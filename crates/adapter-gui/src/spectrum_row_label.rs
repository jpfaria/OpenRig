//! Responsibility: names the stream a spectrum row reads.

use engine::stream_io_labels::StreamIoLabels;

/// `CHAIN  ·  <input> IN <ch>  →  <output> OUT <ch>  ·  <side>` — the binding
/// name plus direction plus 1-based channels on each side, the I/O label every
/// screen uses. A stream with no resolved E/S keeps its 1-based number.
pub fn spectrum_row_label(
    chain_label: &str,
    io: Option<&StreamIoLabels>,
    stream_index: usize,
    side: &str,
) -> String {
    let stream = match io {
        Some(io) => format!(
            "{}  →  {}",
            endpoint(&io.input, "IN", &io.input_channels),
            endpoint(&io.output, "OUT", &io.output_channels)
        ),
        None => format!("STREAM {}", stream_index + 1),
    };
    format!("{}  ·  {stream}  ·  {side}", chain_label.to_uppercase())
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
