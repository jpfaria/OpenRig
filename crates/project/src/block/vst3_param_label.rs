//! Responsibility: turns a VST3 parameter's title into a readable label.
//!
//! Plugins often report ids as titles (`DELAY_MS`, `preDelay`). The label is
//! the plugin's `title` (falling back to `shortTitle`) split on `_` / `-` /
//! camelCase; an ALL-CAPS title becomes sentence case. Words glued without any
//! separator (`LATEDIFFUSIONFEEDBACK`) cannot be split without a dictionary, so
//! they are only case-folded (#1011).

/// `label` without the tab name it repeats: `Node 1: Delay` in the `Node 1`
/// tab reads `Delay`. A label that is only the tab name stays as is.
pub(crate) fn strip_group_prefix(label: &str, group: Option<&str>) -> String {
    let group = group.map(|g| g.trim_end_matches(':').trim()).unwrap_or("");
    if group.is_empty() || label.len() <= group.len() {
        return label.to_string();
    }
    let (head, rest) = label.split_at(group.len());
    let starts_new_word = rest.starts_with(|c: char| c == ':' || c == '-' || c.is_whitespace());
    if !head.eq_ignore_ascii_case(group) || !starts_new_word {
        return label.to_string();
    }
    let stripped = rest.trim_start_matches(|c: char| c == ':' || c == '-' || c.is_whitespace());
    if stripped.is_empty() {
        label.to_string()
    } else {
        capitalize_first(stripped)
    }
}

/// The readable label for a parameter; empty when both titles are blank.
pub(crate) fn humanize_param_label(title: &str, short_title: &str) -> String {
    let source = if title.trim().is_empty() {
        short_title.trim()
    } else {
        title.trim()
    };
    let words: Vec<String> = source
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
        .filter(|w| !w.is_empty())
        .flat_map(split_camel_case)
        .collect();
    let all_caps =
        source.chars().any(char::is_alphabetic) && !source.chars().any(char::is_lowercase);
    let words: Vec<String> = if all_caps {
        words.iter().map(|w| w.to_lowercase()).collect()
    } else {
        words
    };
    capitalize_first(&words.join(" "))
}

/// Split one word at camelCase boundaries: `preDelay` → `pre`, `Delay`;
/// `LFOSpeed` → `LFO`, `Speed` (`LFOs` stays whole). A single leading lowercase letter is a unit
/// prefix, not a word (`dB`, `kHz` stay whole).
fn split_camel_case(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut out = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && c.is_uppercase() {
            let prev = chars[i - 1];
            // A word, not a plural `s`, must follow the acronym (`LFOs` stays).
            let lower_at = |k: usize| chars.get(k).is_some_and(|n| n.is_lowercase());
            let next_is_lower = lower_at(i + 1) && lower_at(i + 2);
            let lower_to_upper = prev.is_lowercase() && current.chars().count() >= 2;
            let acronym_end = prev.is_uppercase() && next_is_lower;
            if lower_to_upper || acronym_end {
                out.push(std::mem::take(&mut current));
            }
        }
        current.push(c);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
#[path = "vst3_param_label_tests.rs"]
mod tests;
