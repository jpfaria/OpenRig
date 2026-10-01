//! Responsibility: offers the split entries of the add-block picker.
//!
//! #328 (spec §5.1, §11): "Split → Mix" and "Y → A/B" follow the block types
//! at every "+", inside a path too, at any depth. The one rule left is local to
//! the list the "+" sits in: a Y ends its own list (the rule
//! `validate_split_layout` enforces), so a Y is offered only where no block
//! but a top-level I/O port follows, and no split lands behind a Y.

use project::block::{AudioBlock, AudioBlockKind, PathRef, SplitEnd};
use project::chain::Chain;

use crate::chain_block_lists::list_at;
use crate::BlockTypePickerItem;

/// `effect_type` of both entries — not a catalog type; the choose-type flow
/// recognises the entries by their position after the block types.
const SPLIT_ENTRY_EFFECT_TYPE: &str = "split";

/// The split ends a block inserted before `position` of the list `path`
/// names may take.
pub(crate) fn split_picker_ends(
    chain: &Chain,
    position: usize,
    path: Option<&PathRef>,
) -> Vec<SplitEnd> {
    let Some(list) = list_at(chain, path) else {
        return Vec::new();
    };
    let position = position.min(list.len());
    let is_y = |b: &AudioBlock| matches!(&b.kind, AudioBlockKind::Split(s) if s.end == SplitEnd::Y);
    if list[..position].iter().any(is_y) {
        return Vec::new();
    }
    let top_level = path.is_none();
    let block_follows = list[position..].iter().any(|b| {
        !(top_level && matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
    });
    let mut ends = vec![SplitEnd::Mix];
    if !block_follows {
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
