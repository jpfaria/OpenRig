//! #787 — geometry of a compact chain row: how its parameter strip wraps into
//! lines, how tall the row is, and where it sits in the flickable.
//!
//! Slint has no flow layout and the compact page positions rows by absolute `y`
//! (drag & drop, drop indicator and insert slots all do arithmetic on it), so
//! the maths lives here and Slint only consumes the result.

use slint::Model;

use crate::{BlockKnobOverlay, BlockParameterItem};

/// Height of one strip line — a parameter cell is 90px tall.
pub(crate) const LINE_HEIGHT_PX: f32 = 90.0;
/// A row that needs no more than one line keeps the historical height.
pub(crate) const BASE_ROW_HEIGHT_PX: f32 = 100.0;
/// The compact tab bar, shown only when the block has 2+ parameter groups.
pub(crate) const TAB_BAR_HEIGHT_PX: f32 = 28.0;
/// Gap between rows (also the insert-slot height).
pub(crate) const ROW_GAP_PX: f32 = 12.0;
/// Vertical padding above/below the strip inside the row.
pub(crate) const ROW_PADDING_PX: f32 = 10.0;
/// Nominal width available to one strip line.
const STRIP_BUDGET_PX: f32 = 720.0;
/// Spacing between cells, mirroring `compact_block_param_strip.slint`.
const CELL_SPACING_PX: f32 = 4.0;
/// Width of a knob cell — the widest kind a curated `knob_layout` produces.
const KNOB_CELL_WIDTH_PX: f32 = 62.0;

/// Cell widths mirror the `preferred-width` of the Slint strip cells. A narrow
/// window shrinks cells to their `min-width` instead of re-wrapping, which keeps
/// the row height (and therefore the drag maths) stable.
fn cell_width_px(it: &BlockParameterItem) -> f32 {
    match it.widget_kind.as_str() {
        "bool" => 48.0,
        // Up to 4 options render as a selector knob; more fall back to the
        // (wider) dropdown.
        "enum" if it.option_labels.row_count() <= 4 => 110.0,
        "enum" => 140.0,
        _ => KNOB_CELL_WIDTH_PX,
    }
}

/// Lay the visible parameters (`tab_slot >= 0`, i.e. the active tab) out into
/// lines, tagging each with its `strip_line`. Hidden parameters keep -1 and take
/// no space. Returns the number of lines used.
pub(crate) fn assign_strip_lines(items: &mut [BlockParameterItem]) -> i32 {
    let mut line = 0i32;
    let mut used = 0.0f32;
    let mut any = false;
    for it in items.iter_mut() {
        if it.tab_slot < 0 {
            it.strip_line = -1;
            continue;
        }
        let width = cell_width_px(it) + CELL_SPACING_PX;
        if any && used + width > STRIP_BUDGET_PX {
            line += 1;
            used = 0.0;
        }
        used += width;
        it.strip_line = line;
        any = true;
    }
    if any {
        line + 1
    } else {
        0
    }
}

/// Same wrap for the curated knob overlays a model's `knob_layout` declares:
/// they replace the generic strip, and every overlay is a knob cell.
pub(crate) fn assign_overlay_lines(overlays: &mut [BlockKnobOverlay]) -> i32 {
    let per_line = (STRIP_BUDGET_PX / (KNOB_CELL_WIDTH_PX + CELL_SPACING_PX)) as usize;
    for (i, knob) in overlays.iter_mut().enumerate() {
        knob.strip_line = (i / per_line) as i32;
    }
    overlays.len().div_ceil(per_line) as i32
}

/// Height of a row whose active tab needs `line_count` strip lines.
pub(crate) fn row_height_px(line_count: i32, has_tabs: bool) -> f32 {
    let tabs = if has_tabs { TAB_BAR_HEIGHT_PX } else { 0.0 };
    let strip = ROW_PADDING_PX + tabs + line_count.max(0) as f32 * LINE_HEIGHT_PX;
    strip.max(BASE_ROW_HEIGHT_PX)
}

/// Absolute `y` of each row inside the flickable viewport: rows are separated by
/// [`ROW_GAP_PX`], which is also the insert slot before the first row.
pub(crate) fn row_y_offsets(heights: &[f32]) -> Vec<f32> {
    let mut y = ROW_GAP_PX;
    heights
        .iter()
        .map(|h| {
            let top = y;
            y += h + ROW_GAP_PX;
            top
        })
        .collect()
}

/// Total height of the flickable viewport: every row, every gap, plus the
/// trailing insert slot.
pub(crate) fn viewport_height_px(heights: &[f32]) -> f32 {
    ROW_GAP_PX + heights.iter().map(|h| h + ROW_GAP_PX).sum::<f32>()
}

#[cfg(test)]
#[path = "compact_block_layout_tests.rs"]
mod tests;
