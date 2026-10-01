//! Responsibility: formats the endpoint text a chain row prints.

pub(crate) fn chain_endpoint_label(prefix: &str, _channels: &[usize]) -> String {
    prefix.to_string()
}

pub(crate) fn format_channel_list(channels: &[usize]) -> String {
    if channels.is_empty() {
        "-".to_string()
    } else {
        channels
            .iter()
            .map(|channel| (channel + 1).to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}
