//! Responsibility: turns the capture names of one tone into the plugin's parameter grid.

use std::collections::BTreeMap;

use plugin_loader::manifest::{GridParameter, ParameterValue};

use super::axis_rows::{direct_rows, preset_rows, AxisKey, Row};
use super::name_tokens::{remove_constant_tokens, tokenize};
use super::natural_sort::sort_values;
use super::token_class::{classify_name, Token};

/// What the captures are: NAM names can carry knob settings, IR names cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureKind {
    Nam,
    Ir,
}

/// The parameters of a plugin and, per capture (same order as the names),
/// its value on each of them.
#[derive(Debug, Clone, PartialEq)]
pub struct Axes {
    pub parameters: Vec<GridParameter>,
    pub values: Vec<BTreeMap<String, ParameterValue>>,
}

pub fn infer_axes(names: &[String], kind: CaptureKind) -> Axes {
    let tokens: Vec<Vec<String>> = names.iter().map(|n| tokenize(n)).collect();
    let residual = remove_constant_tokens(&tokens);
    let classified: Vec<Vec<Token>> = tokens
        .iter()
        .zip(&residual)
        .map(|(all, rest)| classify_name(all, rest, kind == CaptureKind::Nam))
        .collect();
    let knobs = knob_order(&classified);
    let rows = direct_rows(&classified, &knobs).unwrap_or_else(|| preset_rows(&classified, &knobs));
    finish(rows, &knobs)
}

/// Knob names in order of first appearance.
fn knob_order(names: &[Vec<Token>]) -> Vec<&'static str> {
    let mut knobs = Vec::new();
    for token in names.iter().flatten() {
        if let Token::Knob(name, _) = token {
            if !knobs.contains(name) {
                knobs.push(*name);
            }
        }
    }
    knobs
}

/// Drops the axes that do not tell captures apart and names the rest.
fn finish(rows: Vec<Row>, knobs: &[&'static str]) -> Axes {
    let keys: Vec<AxisKey> = rows
        .first()
        .map(|r| r.keys().copied().collect())
        .unwrap_or_default();
    let mut parameters = Vec::new();
    let mut kept = Vec::new();
    for key in keys {
        let mut values: Vec<ParameterValue> = Vec::new();
        for row in &rows {
            if !values.contains(&row[&key]) {
                values.push(row[&key].clone());
            }
        }
        if values.len() < 2 {
            continue;
        }
        sort_values(&mut values);
        let name = axis_name(key, knobs);
        parameters.push(GridParameter {
            display_name: Some(block_core::capitalize_first(&name)),
            name,
            values,
        });
        kept.push(key);
    }
    let values = rows
        .iter()
        .map(|row| {
            kept.iter()
                .map(|key| (axis_name(*key, knobs), row[key].clone()))
                .collect()
        })
        .collect();
    Axes { parameters, values }
}

fn axis_name(key: AxisKey, knobs: &[&'static str]) -> String {
    match key {
        AxisKey::Knob(i) => knobs[i].to_string(),
        AxisKey::Choice(kind) => kind.axis_name().to_string(),
        AxisKey::Preset => "preset".to_string(),
    }
}
