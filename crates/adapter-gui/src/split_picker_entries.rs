//! Responsibility: offers the split entries of the add-block picker.
//!
//! #328 (spec §5.1): "Split → Mix" and "Y → A/B" follow the block types in the
//! picker. A chain holds at most one Mix and one Y, the Mix first, and no split
//! goes inside a path (spec §1.1). A Y must be the chain's last processing
//! block, so it is offered only after the Mix and where no block but an I/O
//! port follows; a Mix only where it lands before the Y (the rules
//! `validate_split_layout` enforces).

use project::block::{find_split_with_end, AudioBlockKind, PathRef, SplitEnd};
use project::chain::Chain;

use crate::BlockTypePickerItem;

/// `effect_type` of both entries — not a catalog type; the choose-type flow
/// recognises the entries by their position after the block types.
const SPLIT_ENTRY_EFFECT_TYPE: &str = "split";

/// The split ends a block inserted before top-level `position` may take.
pub(crate) fn split_picker_ends(
    chain: &Chain,
    position: usize,
    path: Option<&PathRef>,
) -> Vec<SplitEnd> {
    if path.is_some() {
        return Vec::new();
    }
    let mix_at = find_split_with_end(&chain.blocks, SplitEnd::Mix).map(|(at, _)| at);
    let y_at = find_split_with_end(&chain.blocks, SplitEnd::Y).map(|(at, _)| at);
    let mut ends = Vec::new();
    if mix_at.is_none() && y_at.is_none_or(|y| position <= y) {
        ends.push(SplitEnd::Mix);
    }
    let block_follows = chain
        .blocks
        .iter()
        .skip(position)
        .any(|b| !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)));
    if y_at.is_none() && mix_at.is_none_or(|mix| position > mix) && !block_follows {
        ends.push(SplitEnd::Y);
    }
    ends
}

pub(crate) fn split_picker_items(ends: &[SplitEnd]) -> Vec<BlockTypePickerItem> {
    ends.iter()
        .map(|end| {
            let (label, subtitle) = match end {
                SplitEnd::Mix => (
                    rust_i18n::t!("picker-split-mix"),
                    rust_i18n::t!("picker-split-mix-subtitle"),
                ),
                SplitEnd::Y => (
                    rust_i18n::t!("picker-split-y"),
                    rust_i18n::t!("picker-split-y-subtitle"),
                ),
            };
            BlockTypePickerItem {
                effect_type: SPLIT_ENTRY_EFFECT_TYPE.into(),
                label: label.as_ref().into(),
                subtitle: subtitle.as_ref().into(),
                icon_kind: "split".into(),
                use_panel_editor: false,
                uses_model_catalog: false,
                accent_color: crate::ui_state::accent_color_for_icon_kind("split"),
                icon_source: slint::Image::default(),
            }
        })
        .collect()
}

/// The split end a picker row names, when the row is past the `base_len`
/// block types.
pub(crate) fn split_end_for_pick(
    index: usize,
    base_len: usize,
    ends: &[SplitEnd],
) -> Option<SplitEnd> {
    ends.get(index.checked_sub(base_len)?).copied()
}

#[cfg(test)]
#[path = "split_picker_entries_tests.rs"]
mod tests;
