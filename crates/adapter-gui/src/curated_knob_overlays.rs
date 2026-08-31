//! Responsibility: publishes a block's curated knob overlays.

use crate::block_editor_param_items::parameter_groups;
use crate::{BlockKnobOverlay, BlockParameterItem};

/// A curated `knob_layout` names a fixed set of knobs in a fixed order, and the
/// grid draws them by loop index — it knows nothing about parameter tabs. So a
/// block whose parameters form more than one group cannot use it (#915): every
/// tab would draw the same knobs (a native amp's POWER tab listed GAIN and the
/// EQ knobs), and the parameters the layout omits (input, bright, output) would
/// have no control at all. Such a block falls back to the tab-filtered
/// parameter grid, which covers every parameter under its own tab.
fn curated_knobs_apply(param_items: &[BlockParameterItem]) -> bool {
    parameter_groups(param_items).len() <= 1
}

pub(crate) fn build_knob_overlays(
    knob_layout: &[block_core::KnobLayoutEntry],
    param_items: &[BlockParameterItem],
) -> Vec<BlockKnobOverlay> {
    if !curated_knobs_apply(param_items) {
        return Vec::new();
    }
    knob_layout
        .iter()
        .map(|info| {
            let found = param_items
                .iter()
                .find(|p| p.path.as_str() == info.param_key);
            let value = found.map(|p| p.numeric_value).unwrap_or(info.min);
            let label = found
                .map(|p| p.label.to_string().to_uppercase())
                .unwrap_or_else(|| info.param_key.to_uppercase());
            BlockKnobOverlay {
                strip_line: -1,
                path: info.param_key.into(),
                label: label.into(),
                svg_cx: info.svg_cx,
                svg_cy: info.svg_cy,
                svg_r: info.svg_r,
                value,
                min_val: info.min,
                max_val: info.max,
                step: info.step,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "curated_knob_gating_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "issue_915_native_amp_tabs_tests.rs"]
mod editor_tests;
