//! Responsibility: capitalizes a label for display.
//!
//! Not DSP: it sat in `dsp/legacy.rs` only because that file was where the
//! shared helpers landed (#873).

/// Capitalize the first character of a string, leaving the rest unchanged.
pub fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut result = String::with_capacity(s.len());
            for c in first.to_uppercase() {
                result.push(c);
            }
            result.push_str(chars.as_str());
            result
        }
    }
}
