//! Responsibility: classifies each token of a capture name.

use super::enum_tokens::{classify_enum, EnumKind};
use super::knob_tokens::{classify_knob, knob_shaped_count};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A knob and its setting (`mv6` → master 6).
    Knob(&'static str, f64),
    /// A choice axis, its canonical value and the token as written.
    Choice(EnumKind, String, String),
    /// Anything else: part of a preset name.
    Word(String),
}

/// Tags the `residual` tokens of a name. `all` is the name's full token
/// list (single-letter knobs need company there); `read_knobs` is false
/// for IRs, which have no knobs. A knob or choice seen twice in one name
/// reads as a word the second time.
pub fn classify_name(all: &[String], residual: &[String], read_knobs: bool) -> Vec<Token> {
    let with_company = knob_shaped_count(all) >= 2;
    let mut knobs: Vec<&'static str> = Vec::new();
    let mut kinds: Vec<EnumKind> = Vec::new();
    residual
        .iter()
        .map(|token| {
            if let Some((kind, value)) = classify_enum(token) {
                if !kinds.contains(&kind) {
                    kinds.push(kind);
                    return Token::Choice(kind, value, token.clone());
                }
            } else if let Some((knob, value)) = read_knobs
                .then(|| classify_knob(token, with_company))
                .flatten()
            {
                if !knobs.contains(&knob) {
                    knobs.push(knob);
                    return Token::Knob(knob, value);
                }
            }
            Token::Word(token.clone())
        })
        .collect()
}

pub fn knob_value(tokens: &[Token], knob: &str) -> Option<f64> {
    tokens.iter().find_map(|t| match t {
        Token::Knob(name, value) if *name == knob => Some(*value),
        _ => None,
    })
}

pub fn choice_value(tokens: &[Token], kind: EnumKind) -> Option<&str> {
    tokens.iter().find_map(|t| match t {
        Token::Choice(k, value, _) if *k == kind => Some(value.as_str()),
        _ => None,
    })
}
