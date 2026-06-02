//! Pure dimensional layout for the block editor's `BlockPanelEditor`.
//!
//! Wrap policy is universal: every block kind (NAM, native, LV2, IR,
//! VST3) that uses the panel editor must wrap its knobs into multiple
//! rows when the count exceeds `MAX_COLS`. The exact source of the
//! knobs (synthesized LV2 ports vs. curated `knob_layout` overlays)
//! makes no difference — the caller hands us `knob_count` and we lay
//! it out the same way.
//!
//! Living outside Slint so the policy is testable in plain Rust and
//! shared across every code path. Slint reads the result as `in`
//! properties and never re-derives it. Issue #500.

/// Knob cell width — must match `BlockPanelParameterItem`'s natural
/// horizontal footprint.
pub const CELL_WIDTH_PX: f32 = 64.0;

/// Horizontal padding consumed by the inner panel's chrome: 60px on
/// the left (power button) + 8px on the right.
pub const HORIZONTAL_MARGIN_PX: f32 = 68.0;

/// Maximum columns per wrapped row. Lower values force more rows (a
/// 10-knob plugin must wrap into two rows, not stretch into one wide
/// strip).
pub const MAX_COLS: usize = 6;

/// Vertical spacing between rows. Strictly greater than item height so
/// wrapped rows have breathing room.
pub const ROW_STRIDE_PX: f32 = 100.0;

/// Item height (matches `BlockPanelParameterItem` `height: 90px`).
/// Only consumed by the test suite (clip-safety assertion) — production
/// code uses `ROW_STRIDE_PX` instead.
#[allow(dead_code)]
pub const ITEM_HEIGHT_PX: f32 = 90.0;

/// Minimum window width — keeps the header chrome legible even on
/// plugins with one or two knobs.
pub const MIN_PANEL_WIDTH_PX: f32 = 900.0;

/// Baseline outer-window height for a single-row panel.
pub const BASE_PANEL_HEIGHT_PX: f32 = 275.0;

/// Window dimensions when no panel editor is shown (form-based editor).
pub const FORM_EDITOR_WIDTH_PX: f32 = 520.0;
pub const FORM_EDITOR_HEIGHT_PX: f32 = 820.0;

/// Maximum outer-window height (issue #622). The #500 policy grew the
/// window vertically without bound so wrapped rows stayed visible — but
/// the editor window is locked `min = max = preferred = height`, so once
/// the computed height passed the display the lower rows fell off-screen
/// with no way to scroll or shrink. Past this cap the window height stops
/// growing and the param grid scrolls inside the panel instead
/// (`inner_panel_height_px` keeps the full content height as the scroll
/// viewport). Matches the form-editor envelope so both editors fit the
/// same screen budget.
pub const MAX_PANEL_HEIGHT_PX: f32 = 820.0;

/// Inputs to the dimension solver. Every flag that can shift the
/// resulting window size sits here so the test matrix can enumerate
/// combinations explicitly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelInputs {
    /// Number of knobs to render in the grid. Caller picks the
    /// authoritative source: when curated `knob_layout` overlays are
    /// present, use their count; otherwise use `block_parameter_items`
    /// length. EQ blocks (see `has_eq_widget`) pass 0.
    pub knob_count: usize,
    /// `true` when the selected effect type renders a panel-style
    /// editor (Amp, Cab, Dyn, Mod, Filter, …). Native form blocks
    /// fall back to the form editor regardless of knob count.
    pub use_panel_editor: bool,
    /// `true` when an EQ widget (`MultiSliderControl` or
    /// `CurveEditorControl`) is rendered instead of the knob grid.
    pub has_eq_widget: bool,
}

impl PanelInputs {
    #[allow(dead_code)] // used by the unit tests
    pub const fn plain(knob_count: usize) -> Self {
        Self {
            knob_count,
            use_panel_editor: true,
            has_eq_widget: false,
        }
    }
}

/// Computed layout consumed by Slint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelDimensions {
    pub window_width_px: f32,
    pub window_height_px: f32,
    /// Columns occupied by the grid (0 when no grid renders).
    pub grid_cols: usize,
    /// Rows occupied by the grid (0 when no grid renders).
    pub grid_rows: usize,
    /// Inner Rectangle height inside `BlockPanelEditor` — the full
    /// content height of the wrapped knob grid. Grows past
    /// `window_width / 4` with every wrapped row and is NOT capped, so
    /// when it exceeds the (capped) window it doubles as the scroll
    /// viewport content height (issue #622).
    pub inner_panel_height_px: f32,
}

const FORM_FALLBACK: PanelDimensions = PanelDimensions {
    window_width_px: FORM_EDITOR_WIDTH_PX,
    window_height_px: FORM_EDITOR_HEIGHT_PX,
    grid_cols: 0,
    grid_rows: 0,
    inner_panel_height_px: 0.0,
};

/// Number of columns the wrapped grid will occupy for `knob_count`.
pub fn grid_cols(knob_count: usize) -> usize {
    knob_count.min(MAX_COLS)
}

/// Number of rows the wrapped grid will occupy — int-safe ceil so the
/// formula matches Slint's truncating integer division.
pub fn grid_rows(knob_count: usize) -> usize {
    let cols = grid_cols(knob_count);
    if cols == 0 {
        0
    } else {
        ((knob_count - 1) / cols) + 1
    }
}

/// Solve dimensions for a given input. Pure function — every Slint
/// branch maps to a path here, every path is unit-tested below.
pub fn compute(inputs: PanelInputs) -> PanelDimensions {
    if !inputs.use_panel_editor {
        return FORM_FALLBACK;
    }
    if inputs.has_eq_widget {
        return PanelDimensions {
            window_width_px: MIN_PANEL_WIDTH_PX,
            window_height_px: BASE_PANEL_HEIGHT_PX,
            grid_cols: 0,
            grid_rows: 0,
            inner_panel_height_px: MIN_PANEL_WIDTH_PX / 4.0,
        };
    }

    let cols = grid_cols(inputs.knob_count);
    let rows = grid_rows(inputs.knob_count);
    let needed_width = if cols == 0 {
        MIN_PANEL_WIDTH_PX
    } else {
        cols as f32 * CELL_WIDTH_PX + HORIZONTAL_MARGIN_PX
    };
    let window_width = needed_width.max(MIN_PANEL_WIDTH_PX);
    let extra_rows = rows.saturating_sub(1) as f32;
    // The window height is capped (issue #622): once the grid is taller
    // than the screen budget the window stops growing and the grid scrolls
    // inside it. `inner_panel_height` is NOT capped — it stays the full
    // content height so it can serve as the scroll viewport.
    let content_height = BASE_PANEL_HEIGHT_PX + extra_rows * ROW_STRIDE_PX;
    let window_height = content_height.min(MAX_PANEL_HEIGHT_PX);
    let base_inner = window_width / 4.0;
    let inner_panel_height = base_inner + extra_rows * ROW_STRIDE_PX;

    PanelDimensions {
        window_width_px: window_width,
        window_height_px: window_height,
        grid_cols: cols,
        grid_rows: rows,
        inner_panel_height_px: inner_panel_height,
    }
}

#[cfg(test)]
#[path = "block_panel_dimensions_tests.rs"]
mod tests;
