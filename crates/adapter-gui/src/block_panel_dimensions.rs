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

/// Header height of `BlockPanelEditor` — match `header-height` in
/// `block_panel_editor.slint`.
pub const HEADER_HEIGHT_PX: f32 = 48.0;

/// Height of a `CurveEditorControl`. 280px (not the 200px it started at) so a
/// band dragged to the bottom of the -24..+24 dB range still clears its
/// frequency label (issue #358).
pub const CURVE_EDITOR_HEIGHT_PX: f32 = 280.0;

/// Height of a `MultiSliderControl` — same reasoning, narrower range.
pub const MULTI_SLIDER_HEIGHT_PX: f32 = 240.0;

/// Horizontal room one band takes in each EQ widget, plus the chrome around
/// the row. A curve editor gives each band a draggable node, so it needs far
/// more width per band than a fixed-frequency slider.
pub const CURVE_EDITOR_BAND_WIDTH_PX: f32 = 200.0;
pub const MULTI_SLIDER_BAND_WIDTH_PX: f32 = 35.0;
pub const EQ_WIDTH_CHROME_PX: f32 = 80.0;
pub const EQ_MIN_WIDTH_PX: f32 = 600.0;
pub const CURVE_EDITOR_MAX_WIDTH_PX: f32 = 1200.0;
pub const MULTI_SLIDER_MAX_WIDTH_PX: f32 = 1400.0;

/// Vertical breathing room around the EQ widget (8px above, 8px below) —
/// match its `y` offset and height subtraction in Slint.
pub const EQ_WIDGET_MARGIN_PX: f32 = 16.0;

/// EQ strip holding the power switch alone.
pub const EQ_STRIP_BARE_PX: f32 = 56.0;

/// EQ strip holding the power switch plus the knobs the widget does not draw
/// — one parameter cell tall.
pub const EQ_STRIP_WITH_KNOBS_PX: f32 = 92.0;

/// Window dimensions when no panel editor is shown (form-based editor).
pub const FORM_EDITOR_WIDTH_PX: f32 = 520.0;
pub const FORM_EDITOR_HEIGHT_PX: f32 = 820.0;

/// Per-parameter row height in the FORM editor — `BlockParameterRow` is
/// 96px tall (`block_editor_panel.slint`). The form window grows by this
/// much per parameter so every row is visible without scrolling: the
/// editor window is locked `min = max = preferred`, so a fixed height left
/// the lower params unreachable (issue #622).
pub const FORM_ROW_PX: f32 = 96.0;

/// Fixed vertical chrome above + below the form editor's parameter list
/// (model picker, header, margins). The form window must be at least this
/// tall plus `param_count * FORM_ROW_PX` to show every parameter.
pub const FORM_CHROME_PX: f32 = 164.0;

/// The EQ widget a block renders instead of the knob grid, with the number of
/// bands it draws — both drive the window it needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EqWidget {
    None,
    CurveEditor { bands: usize },
    MultiSlider { bands: usize },
}

impl EqWidget {
    pub const fn is_some(self) -> bool {
        !matches!(self, Self::None)
    }

    /// Height the widget draws at.
    const fn height_px(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::CurveEditor { .. } => CURVE_EDITOR_HEIGHT_PX,
            Self::MultiSlider { .. } => MULTI_SLIDER_HEIGHT_PX,
        }
    }

    /// Width the widget needs for its bands, clamped to a sane window.
    fn width_px(self) -> f32 {
        let (bands, per_band, max) = match self {
            Self::None => return MIN_PANEL_WIDTH_PX,
            Self::CurveEditor { bands } => {
                (bands, CURVE_EDITOR_BAND_WIDTH_PX, CURVE_EDITOR_MAX_WIDTH_PX)
            }
            Self::MultiSlider { bands } => {
                (bands, MULTI_SLIDER_BAND_WIDTH_PX, MULTI_SLIDER_MAX_WIDTH_PX)
            }
        };
        (bands as f32 * per_band + EQ_WIDTH_CHROME_PX).clamp(EQ_MIN_WIDTH_PX, max)
    }
}

/// Inputs to the dimension solver. Every flag that can shift the
/// resulting window size sits here so the test matrix can enumerate
/// combinations explicitly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelInputs {
    /// Number of knobs to render in the grid. Caller picks the
    /// authoritative source: when curated `knob_layout` overlays are
    /// present, use their count; otherwise use `block_parameter_items`
    /// length. EQ blocks (see `eq_widget`) pass the knobs their widget
    /// does not draw.
    pub knob_count: usize,
    /// `true` when the selected effect type renders a panel-style
    /// editor (Amp, Cab, Dyn, Mod, Filter, …). Native form blocks
    /// fall back to the form editor regardless of knob count.
    pub use_panel_editor: bool,
    /// The EQ widget rendered instead of the knob grid, if any.
    pub eq_widget: EqWidget,
}

impl PanelInputs {
    #[allow(dead_code)] // used by the unit tests
    pub const fn plain(knob_count: usize) -> Self {
        Self {
            knob_count,
            use_panel_editor: true,
            eq_widget: EqWidget::None,
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
    /// Inner Rectangle height inside `BlockPanelEditor`. Grows past
    /// `window_width / 4` so wrapped rows stay inside the clipped
    /// panel rectangle.
    pub inner_panel_height_px: f32,
}

/// Form-editor (non-panel) window dimensions. Width is fixed; height grows
/// with the parameter count so every row shows without scrolling, floored
/// at `FORM_EDITOR_HEIGHT_PX` so small forms keep the baseline envelope.
fn form_dimensions(param_count: usize) -> PanelDimensions {
    let needed_height = FORM_CHROME_PX + param_count as f32 * FORM_ROW_PX;
    PanelDimensions {
        window_width_px: FORM_EDITOR_WIDTH_PX,
        window_height_px: FORM_EDITOR_HEIGHT_PX.max(needed_height),
        grid_cols: 0,
        grid_rows: 0,
        inner_panel_height_px: 0.0,
    }
}

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

/// Pick the widget from the two point models the editor publishes — the curve
/// editor wins when both carry rows, exactly as the Slint branches do.
pub fn eq_widget_for(curve_points: usize, slider_points: usize) -> EqWidget {
    if curve_points > 0 {
        EqWidget::CurveEditor {
            bands: curve_points,
        }
    } else if slider_points > 0 {
        EqWidget::MultiSlider {
            bands: slider_points,
        }
    } else {
        EqWidget::None
    }
}

/// Solve dimensions for a given input. Pure function — every Slint
/// branch maps to a path here, every path is unit-tested below.
pub fn compute(inputs: PanelInputs) -> PanelDimensions {
    if !inputs.use_panel_editor {
        return form_dimensions(inputs.knob_count);
    }
    if inputs.eq_widget.is_some() {
        // The strip above the widget holds the power switch, plus one cell of
        // knobs when the widget leaves any for the grid (#878).
        let strip = if inputs.knob_count > 0 {
            EQ_STRIP_WITH_KNOBS_PX
        } else {
            EQ_STRIP_BARE_PX
        };
        return PanelDimensions {
            window_width_px: inputs.eq_widget.width_px(),
            // Exactly what the window stacks: the widget sits below the header
            // and the strip, so a window sized for anything else cuts off its
            // bottom row of frequency labels (#878, #358).
            window_height_px: HEADER_HEIGHT_PX
                + strip
                + inputs.eq_widget.height_px()
                + EQ_WIDGET_MARGIN_PX,
            grid_cols: 0,
            grid_rows: 0,
            inner_panel_height_px: strip,
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
    // Grow the window vertically to fit every wrapped row (issue #500 / #622):
    // the editor window is locked min=max=preferred, so it must be sized to
    // show all knobs — never capped (a cap would clip the lower rows).
    let window_height = BASE_PANEL_HEIGHT_PX + extra_rows * ROW_STRIDE_PX;
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
