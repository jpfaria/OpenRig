//! Responsibility: assigns every capture its value on each candidate axis.
//!
//! Two shapes, as in the curated manifests: when every token is a knob or a
//! choice and they tell the captures apart, those are the axes (a knob a
//! capture lacks reads -1, a missing choice reads `none`); otherwise the
//! knobs and choices every capture carries stay, and the rest of each name
//! becomes a `preset` value.

use std::collections::BTreeMap;

use plugin_loader::manifest::ParameterValue;

use super::enum_tokens::EnumKind;
use super::name_tokens::is_number;
use super::token_class::{choice_value, knob_value, Token};

/// Axis identity; the derived order is the manifest's axis order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AxisKey {
    /// Index into the knob list, in order of first appearance.
    Knob(usize),
    Choice(EnumKind),
    Preset,
}

pub type Row = BTreeMap<AxisKey, ParameterValue>;

pub fn direct_rows(names: &[Vec<Token>], knobs: &[&'static str]) -> Option<Vec<Row>> {
    if names.iter().flatten().any(|t| matches!(t, Token::Word(_))) {
        return None;
    }
    let kinds: Vec<EnumKind> = EnumKind::ALL
        .into_iter()
        .filter(|kind| names.iter().any(|n| choice_value(n, *kind).is_some()))
        .collect();
    let rows: Vec<Row> = names
        .iter()
        .map(|tokens| {
            let mut row = Row::new();
            for (i, knob) in knobs.iter().enumerate() {
                let value = knob_value(tokens, knob).unwrap_or(-1.0);
                row.insert(AxisKey::Knob(i), ParameterValue::Number(value));
            }
            for kind in &kinds {
                let value = choice_value(tokens, *kind).unwrap_or("none");
                row.insert(AxisKey::Choice(*kind), ParameterValue::Text(value.into()));
            }
            row
        })
        .collect();
    all_distinct(&rows).then_some(rows)
}

pub fn preset_rows(names: &[Vec<Token>], knobs: &[&'static str]) -> Vec<Row> {
    let everywhere_knob = |knob: &str| names.iter().all(|n| knob_value(n, knob).is_some());
    let everywhere_kind = |kind| names.iter().all(|n| choice_value(n, kind).is_some());
    let kept_knobs: Vec<usize> = (0..knobs.len())
        .filter(|&i| everywhere_knob(knobs[i]) && varies(names, |n| knob_value(n, knobs[i])))
        .collect();
    let kept_kinds: Vec<EnumKind> = EnumKind::ALL
        .into_iter()
        .filter(|&kind| {
            everywhere_kind(kind) && varies(names, |n| choice_value(n, kind).map(str::to_owned))
        })
        .collect();

    let labels: Vec<String> = names
        .iter()
        .map(|tokens| {
            let words: Vec<&str> = tokens
                .iter()
                .filter_map(|t| match t {
                    Token::Word(word) => Some(word.as_str()),
                    Token::Choice(kind, _, raw) if !everywhere_kind(*kind) => Some(raw.as_str()),
                    _ => None,
                })
                .collect();
            if words.is_empty() {
                "default".to_string()
            } else {
                words.join("_")
            }
        })
        .collect();
    let numeric = labels.iter().all(|l| is_number(l));

    let mut rows: Vec<Row> = names
        .iter()
        .zip(&labels)
        .map(|(tokens, label)| {
            let mut row = Row::new();
            for &i in &kept_knobs {
                let value = knob_value(tokens, knobs[i]).unwrap_or(-1.0);
                row.insert(AxisKey::Knob(i), ParameterValue::Number(value));
            }
            for &kind in &kept_kinds {
                let value = choice_value(tokens, kind).unwrap_or("none");
                row.insert(AxisKey::Choice(kind), ParameterValue::Text(value.into()));
            }
            let preset = if numeric {
                ParameterValue::Number(label.parse().unwrap_or(0.0))
            } else {
                ParameterValue::Text(label.clone())
            };
            row.insert(AxisKey::Preset, preset);
            row
        })
        .collect();
    if !all_distinct(&rows) {
        for (i, row) in rows.iter_mut().enumerate() {
            row.insert(AxisKey::Preset, ParameterValue::Number((i + 1) as f64));
        }
    }
    rows
}

fn varies<T: PartialEq>(names: &[Vec<Token>], value: impl Fn(&[Token]) -> Option<T>) -> bool {
    let first = names.first().and_then(|n| value(n));
    names.iter().any(|n| value(n) != first)
}

fn all_distinct(rows: &[Row]) -> bool {
    rows.iter()
        .enumerate()
        .all(|(i, row)| !rows[..i].contains(row))
}
