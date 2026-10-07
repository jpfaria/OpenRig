//! Responsibility: reads knob settings out of capture-name tokens.
//!
//! `mv6` → master 6, `g7` → gain 7, `gain7.5` → gain 7.5, `7g` → gain 7.
//! Single-letter abbreviations other than `g` are ambiguous (`v30` is a
//! speaker), so they only count when the name carries another knob token.

use super::name_tokens::is_number;

/// Abbreviation → knob name.
const KNOBS: &[(&str, &str)] = &[
    ("gain", "gain"),
    ("drive", "drive"),
    ("tone", "tone"),
    ("level", "level"),
    ("volume", "volume"),
    ("master", "master"),
    ("treble", "treble"),
    ("bass", "bass"),
    ("mid", "mid"),
    ("mids", "mid"),
    ("middle", "mid"),
    ("presence", "presence"),
    ("depth", "depth"),
    ("reverb", "reverb"),
    ("sustain", "sustain"),
    ("contour", "contour"),
    ("output", "output"),
    ("g", "gain"),
    ("mv", "master"),
    ("p", "presence"),
    ("b", "bass"),
    ("m", "mid"),
    ("t", "treble"),
    ("v", "volume"),
    ("vol", "volume"),
    ("pres", "presence"),
    ("treb", "treble"),
    ("lvl", "level"),
    ("dr", "drive"),
];

/// How many tokens of a name look like `<abbreviation><number>`.
pub fn knob_shaped_count(tokens: &[String]) -> usize {
    tokens.iter().filter(|t| abbreviated(t).is_some()).count()
}

/// The knob a token sets and its value. `with_company` is whether the name
/// has at least two knob-shaped tokens.
pub fn classify_knob(token: &str, with_company: bool) -> Option<(&'static str, f64)> {
    if let Some((abbr, knob, value)) = abbreviated(token) {
        if abbr.len() > 1 || abbr == "g" || with_company {
            return Some((knob, value));
        }
        return None;
    }
    let setting = token.strip_suffix('g')?;
    is_number(setting).then(|| ("gain", setting.parse().unwrap_or(0.0)))
}

fn abbreviated(token: &str) -> Option<(&str, &'static str, f64)> {
    let split = token.find(|c: char| c.is_ascii_digit())?;
    let (abbr, setting) = token.split_at(split);
    if !is_number(setting) {
        return None;
    }
    let knob = KNOBS.iter().find(|(a, _)| *a == abbr)?.1;
    Some((abbr, knob, setting.parse().ok()?))
}
