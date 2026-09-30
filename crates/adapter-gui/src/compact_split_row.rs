//! Responsibility: builds the compact view row of a split.
//!
//! #328: a split has no model and no E/S, so its row carries only what it is —
//! the split icon and its kind (SPLIT MIX / SPLIT Y) — with no model selector.
//! Its paths follow it as their own rows (`compact_row_address`).

use std::rc::Rc;

use slint::{ModelRc, SharedString, VecModel};

use project::block::{AudioBlock, SplitBlock, SplitEnd};

use crate::compact_block_layout::row_height_px;
use crate::CompactBlockItem;

/// The icon set and the accent table both know the split by this name.
const SPLIT_ICON_KIND: &str = "split";

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
        row_height: row_height_px(1, false),
        ..Default::default()
    }
}
