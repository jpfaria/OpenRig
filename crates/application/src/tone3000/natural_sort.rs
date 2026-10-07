//! Responsibility: orders axis values the way a person reads them.
//!
//! Numbers by value; text with embedded numbers by those numbers
//! (`egnl2` before `egnl10`); numbers before text.

use std::cmp::Ordering;

use plugin_loader::manifest::ParameterValue;

pub fn sort_values(values: &mut [ParameterValue]) {
    values.sort_by(compare_values);
}

pub fn compare_values(a: &ParameterValue, b: &ParameterValue) -> Ordering {
    match (a, b) {
        (ParameterValue::Number(x), ParameterValue::Number(y)) => x.total_cmp(y),
        (ParameterValue::Text(x), ParameterValue::Text(y)) => natural_cmp(x, y),
        _ => rank(a).cmp(&rank(b)),
    }
}

pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (chunks(a), chunks(b));
    loop {
        match (x.next(), y.next()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(l), Some(r)) => {
                let order = match (digits(l), digits(r)) {
                    (true, true) => compare_digits(l, r),
                    _ => l.cmp(r),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

fn rank(value: &ParameterValue) -> u8 {
    match value {
        ParameterValue::Bool(_) => 0,
        ParameterValue::Number(_) => 1,
        ParameterValue::Text(_) => 2,
    }
}

fn digits(chunk: &str) -> bool {
    chunk.bytes().all(|b| b.is_ascii_digit())
}

fn compare_digits(l: &str, r: &str) -> Ordering {
    let (l, r) = (l.trim_start_matches('0'), r.trim_start_matches('0'));
    l.len().cmp(&r.len()).then_with(|| l.cmp(r))
}

/// Runs of digits and runs of anything else.
fn chunks(text: &str) -> impl Iterator<Item = &str> {
    let bytes = text.as_bytes();
    let mut start = 0;
    std::iter::from_fn(move || {
        if start >= bytes.len() {
            return None;
        }
        let kind = bytes[start].is_ascii_digit();
        let end = (start..bytes.len())
            .find(|&i| bytes[i].is_ascii_digit() != kind)
            .unwrap_or(bytes.len());
        let chunk = &text[start..end];
        start = end;
        Some(chunk)
    })
}
