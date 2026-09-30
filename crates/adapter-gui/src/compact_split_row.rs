//! Responsibility: builds the compact view row of a split.
//!
//! #328: a split has no model and no E/S, so its row carries the split icon,
//! its kind (SPLIT MIX / SPLIT Y) and the split editor's knobs — with no model
//! selector. A Mix split also shows its mixer, as a second tab. Its paths
//! follow it as their own rows (`compact_row_address`).

use std::rc::Rc;

use slint::{ModelRc, SharedString, VecModel};

use project::block::{AudioBlock, SplitBlock, SplitEnd};

use crate::compact_block_layout::{assign_strip_lines, row_height_px};
use crate::compact_block_tabs::active_group_index;
use crate::compact_block_view::param_lines;
use crate::param_tab_grouping::{retag_all, retag_for_group, tab_groups};
use crate::split_editor_items::{split_editor_items, SplitEditorKind};
use crate::{BlockParameterItem, CompactBlockItem};

/// The icon set and the accent table both know the split by this name.
const SPLIT_ICON_KIND: &str = "split";

/// The split editor's knobs, then the mixer's when the paths mix back.
fn split_knobs(split: &SplitBlock) -> Vec<BlockParameterItem> {
    let mut items = split_editor_items(split, SplitEditorKind::Split);
    if split.end == SplitEnd::Mix {
        items.extend(split_editor_items(split, SplitEditorKind::Mixer));
    }
    items
}

pub(crate) fn split_compact_item(
    chain_index: usize,
    row: usize,
    block: &AudioBlock,
    split: &SplitBlock,
) -> CompactBlockItem {
    let label = match split.end {
        SplitEnd::Mix => rust_i18n::t!("picker-split-mix"),
        SplitEnd::Y => rust_i18n::t!("picker-split-y"),
    };
    let accent = crate::ui_state::accent_color_for_icon_kind(SPLIT_ICON_KIND);

    let knobs = split_knobs(split);
    let groups = tab_groups(&knobs);
    let active_index = active_group_index(&block.id.0, &groups);
    let mut params = match groups.get(active_index) {
        Some(active) if groups.len() > 1 => retag_for_group(&knobs, active),
        _ => retag_all(&knobs),
    };
    let lines = assign_strip_lines(&mut params);
    let has_tabs = groups.len() > 1;
    let cell_lines = param_lines(&params, lines);

    CompactBlockItem {
        chain_index: chain_index as i32,
        block_index: row as i32,
        block_id: block.id.0.clone().into(),
        effect_type: SPLIT_ICON_KIND.into(),
        model_id: split.end.as_str().into(),
        icon_kind: SPLIT_ICON_KIND.into(),
        display_label: SharedString::from(label.as_ref()),
        enabled: block.enabled,
        panel_bg: slint::Color::from_argb_u8(0xff, 0x17, 0x1a, 0x20),
        panel_text: accent,
        accent_color: accent,
        model_labels: ModelRc::from(Rc::new(VecModel::<SharedString>::default())),
        model_selected_index: -1,
        parameter_items: ModelRc::from(Rc::new(VecModel::from(params))),
        parameter_groups: ModelRc::from(Rc::new(VecModel::from(
            groups
                .iter()
                .map(|g| SharedString::from(g.as_str()))
                .collect::<Vec<_>>(),
        ))),
        active_parameter_group: active_index as i32,
        parameter_lines: ModelRc::from(Rc::new(VecModel::from(cell_lines))),
        row_height: row_height_px(lines.max(1), has_tabs),
        ..Default::default()
    }
}
