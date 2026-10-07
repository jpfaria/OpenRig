//! Responsibility: splits capture names into comparable tokens.
//!
//! Same normalisation the curated OpenRig-plugins manifests were written
//! with: lowercase words, digits without leading zeros, a knob word glued to
//! its setting (`gain 7.5` → `gain7.5`) and two-word mic positions kept as
//! one token (`cap edge` → `cap_edge`).

use std::collections::HashMap;

/// Words that name a knob when a number follows them.
const KNOB_WORDS: &[&str] = &[
    "gain", "drive", "tone", "level", "volume", "master", "treble", "bass", "mid", "mids",
    "middle", "presence", "depth", "reverb", "sustain", "contour", "output", "mv", "vol", "pres",
    "treb",
];

pub fn tokenize(name: &str) -> Vec<String> {
    merge_phrases(split_words(&name.to_lowercase()))
}

/// Drops the tokens every name shares (as a multiset: a token twice in
/// every name drops twice), leaving only what tells the captures apart.
pub fn remove_constant_tokens(names: &[Vec<String>]) -> Vec<Vec<String>> {
    let Some((first, rest)) = names.split_first() else {
        return Vec::new();
    };
    let mut common = counts(first);
    for name in rest {
        let here = counts(name);
        common.retain(|token, n| {
            *n = (*n).min(here.get(token).copied().unwrap_or(0));
            *n > 0
        });
    }
    names
        .iter()
        .map(|name| {
            let mut left = common.clone();
            name.iter()
                .filter(|token| match left.get_mut(token.as_str()) {
                    Some(n) if *n > 0 => {
                        *n -= 1;
                        false
                    }
                    _ => true,
                })
                .cloned()
                .collect()
        })
        .collect()
}

pub fn is_number(token: &str) -> bool {
    let mut parts = token.splitn(2, '.');
    let whole = parts.next().unwrap_or("");
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    digits(whole) && parts.next().is_none_or(digits)
}

fn counts(tokens: &[String]) -> HashMap<&str, usize> {
    let mut map = HashMap::new();
    for token in tokens {
        *map.entry(token.as_str()).or_insert(0) += 1;
    }
    map
}

/// Splits on anything not alphanumeric, keeping a `.` between two digits.
fn split_words(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut words = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let decimal_point = c == '.'
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if c.is_alphanumeric() || decimal_point {
            current.push(c);
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words.into_iter().map(strip_leading_zeros).collect()
}

fn strip_leading_zeros(word: String) -> String {
    if !word.is_empty() && word.bytes().all(|b| b.is_ascii_digit()) {
        let trimmed = word.trim_start_matches('0');
        if trimmed.is_empty() {
            "0".to_string()
        } else {
            trimmed.to_string()
        }
    } else {
        word
    }
}

fn merge_phrases(words: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let word = &words[i];
        let next = words.get(i + 1).map(String::as_str);
        let merged = match next {
            Some(n) if KNOB_WORDS.contains(&word.as_str()) && is_number(n) => {
                Some(format!("{word}{n}"))
            }
            Some("edge") if word == "cap" || word == "cone" => Some(format!("{word}_edge")),
            Some("inch" | "in") if word.bytes().all(|b| b.is_ascii_digit()) => {
                Some(format!("{word}_inch"))
            }
            _ => None,
        };
        match merged {
            Some(token) => {
                out.push(token);
                i += 2;
            }
            None => {
                out.push(word.clone());
                i += 1;
            }
        }
    }
    out
}
