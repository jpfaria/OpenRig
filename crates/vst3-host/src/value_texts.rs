//! Responsibility: samples a continuous VST3 parameter's display text per knob position.
//!
//! OpenRig drives a continuous VST3 parameter with a 0–100 knob in steps of 1,
//! so reading the plugin's own `getParamStringByValue` text at those 101
//! positions gives the exact label for every value the knob can take (#1011).

/// Knob positions of a continuous VST3 parameter (0..=100, step 1).
pub const VALUE_TEXT_POSITIONS: usize = 101;

/// The plugin's text at each knob position, or empty when the plugin formats
/// no value at all (the knob then shows its number).
pub(crate) fn continuous_value_texts(
    units: &str,
    mut read: impl FnMut(f64) -> Option<String>,
) -> Vec<String> {
    let units = units.trim();
    let texts: Vec<String> = (0..VALUE_TEXT_POSITIONS)
        .map(|k| {
            let text = read(k as f64 / (VALUE_TEXT_POSITIONS - 1) as f64)
                .map(|t| t.trim().to_string())
                .unwrap_or_default();
            if text.is_empty() || units.is_empty() || text.ends_with(units) {
                text
            } else {
                format!("{text} {units}")
            }
        })
        .collect();
    if texts.iter().all(String::is_empty) {
        Vec::new()
    } else {
        texts
    }
}

#[cfg(test)]
#[path = "value_texts_tests.rs"]
mod tests;
