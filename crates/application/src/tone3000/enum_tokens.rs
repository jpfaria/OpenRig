//! Responsibility: recognises the categorical words of a capture name.

/// Choice axes a capture name can carry, in manifest axis order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EnumKind {
    Mic,
    Position,
    Speaker,
    Voicing,
}

impl EnumKind {
    pub const ALL: [EnumKind; 4] = [Self::Mic, Self::Position, Self::Speaker, Self::Voicing];

    pub const fn axis_name(self) -> &'static str {
        match self {
            Self::Mic => "mic",
            Self::Position => "position",
            Self::Speaker => "speaker",
            Self::Voicing => "voicing",
        }
    }
}

/// Mics matched by prefix (`md421k` → `md421`), longest first wins.
const MICS: &[&str] = &[
    "sm57", "sm7b", "md421", "re20", "beta52", "c414", "r121", "r10", "m160", "u87",
];
const SPEAKER: &[&str] = &["upper", "lower"];
const VOICING: &[&str] = &["hgt", "hg", "normal", "clean", "crunch", "lead"];
const POSITION: &[&str] = &["cap_edge", "cone_edge", "cap", "cone", "distant"];

/// The choice a token names, with its canonical value.
pub fn classify_enum(token: &str) -> Option<(EnumKind, String)> {
    if let Some(mic) = MICS
        .iter()
        .filter(|mic| token.starts_with(*mic))
        .max_by_key(|mic| mic.len())
    {
        return Some((EnumKind::Mic, (*mic).to_string()));
    }
    if POSITION.contains(&token) {
        return Some((EnumKind::Position, token.to_string()));
    }
    if let Some(distance) = mic_distance(token) {
        return Some((EnumKind::Position, distance));
    }
    if SPEAKER.contains(&token) {
        return Some((EnumKind::Speaker, token.to_string()));
    }
    if VOICING.contains(&token) {
        return Some((EnumKind::Voicing, token.to_string()));
    }
    None
}

/// `12in` / `12_inch` → `12_inch`.
fn mic_distance(token: &str) -> Option<String> {
    let digits = token
        .strip_suffix("_inch")
        .or_else(|| token.strip_suffix("in"))?;
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| format!("{digits}_inch"))
}
