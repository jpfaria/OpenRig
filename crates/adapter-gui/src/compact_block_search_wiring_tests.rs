//! Issue #537: the compact block editor's search popup never narrows
//! the model list. The wiring (`compact_chain_callbacks::wire` calls
//! `refilter_compact_block`) is correct end-to-end at the Rust level
//! — these tests pin that — but the Slint binding chain
//!   `for block-item in compact-blocks
//!      → CompactBlockRow.block-data
//!      → ModelSelectWithSearch.filtered-models
//!         = block-data.filtered-models`
//! never re-evaluates the inner ModelRc when only the underlying
//! VecModel mutates. The fix is to nudge the outer `compact_blocks`
//! VecModel via `set_row_data` carrying the SAME row struct (and
//! therefore the SAME `filtered_models` Rc), so the popup keeps
//! observing the same VecModel (now filtered).

use crate::model_search_wiring::refilter_compact_block;
use crate::{BlockModelPickerItem, CompactBlockItem};
use slint::{Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

fn picker_item(model_id: &str, display: &str, brand: &str) -> BlockModelPickerItem {
    BlockModelPickerItem {
        effect_type: "preamp".into(),
        model_id: model_id.into(),
        label: display.into(),
        display_name: display.into(),
        subtitle: SharedString::default(),
        icon_kind: "preamp".into(),
        brand: brand.into(),
        type_label: "NATIVE".into(),
        panel_bg: slint::Color::from_argb_u8(255, 0, 0, 0),
        panel_text: slint::Color::from_argb_u8(255, 255, 255, 255),
        brand_strip_bg: slint::Color::from_argb_u8(255, 0, 0, 0),
        model_font: SharedString::default(),
        available: true,
        thumbnail_path: SharedString::default(),
    }
}

fn preamp_compact_block(chain_index: i32, block_index: i32) -> CompactBlockItem {
    let models = vec![
        picker_item("american_clean", "American Clean", "fender"),
        picker_item("brit_crunch", "Brit Crunch", "marshall"),
        picker_item("modern_high_gain", "Modern High Gain", "mesa"),
    ];
    CompactBlockItem {
        chain_index,
        block_index,
        models: ModelRc::from(Rc::new(VecModel::from(models.clone()))),
        filtered_models: ModelRc::from(Rc::new(VecModel::from(models))),
        ..Default::default()
    }
}

fn block_filtered_display_names(item: &CompactBlockItem) -> Vec<String> {
    (0..item.filtered_models.row_count())
        .filter_map(|i| {
            item.filtered_models
                .row_data(i)
                .map(|m| m.display_name.into())
        })
        .collect()
}

// ── In-isolation behavior of the helper ────────────────────────────────

#[test]
fn refilter_narrows_filtered_models_of_the_addressed_block() {
    let compact_blocks = Rc::new(VecModel::from(vec![
        preamp_compact_block(0, 0),
        preamp_compact_block(0, 1),
    ]));

    refilter_compact_block(&compact_blocks, 0, 0, "brit");

    let row0 = compact_blocks.row_data(0).expect("row 0");
    assert_eq!(
        block_filtered_display_names(&row0),
        vec!["Brit Crunch".to_string()],
        "block (0, 0) must be narrowed to the single 'brit' match"
    );
    let row1 = compact_blocks.row_data(1).expect("row 1");
    assert_eq!(
        block_filtered_display_names(&row1).len(),
        3,
        "the sibling block (0, 1) must not be touched by a search on (0, 0)"
    );
}

#[test]
fn refilter_empty_query_restores_the_full_list() {
    let compact_blocks = Rc::new(VecModel::from(vec![preamp_compact_block(0, 0)]));

    refilter_compact_block(&compact_blocks, 0, 0, "brit");
    refilter_compact_block(&compact_blocks, 0, 0, "");

    let row = compact_blocks.row_data(0).expect("row 0");
    assert_eq!(
        block_filtered_display_names(&row).len(),
        3,
        "an empty query must restore the full 3-model list"
    );
}

// ── Wiring presence (source-presence test, same style as
//     `tests/chain_row_height_grows_with_streams.rs`) ─────────────────────

#[test]
fn compact_chain_callbacks_wires_on_search_block_model_to_refilter() {
    use std::path::PathBuf;

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/compact_chain_callbacks.rs");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    assert!(
        src.contains("on_search_block_model"),
        "compact_chain_callbacks.rs must register a handler for the compact \
         window's 3-arg `search-block-model` callback"
    );
    assert!(
        src.contains("refilter_compact_block"),
        "compact_chain_callbacks.rs must forward the search-block-model \
         callback to `refilter_compact_block`"
    );
}

// ── #537 wiring contract — refilter MUST keep the SAME `filtered_models`
//     ModelRc on the row after filtering, so the open popup (already
//     bound to that exact ModelRc) keeps observing it. Replacing the Rc
//     orphans the popup's binding and the user sees stale data.

#[test]
fn refilter_keeps_the_same_filtered_models_modelrc_so_the_popup_keeps_observing() {
    let compact_blocks = Rc::new(VecModel::from(vec![preamp_compact_block(0, 0)]));
    let popup_rc = compact_blocks.row_data(0).expect("row 0").filtered_models;

    refilter_compact_block(&compact_blocks, 0, 0, "brit");

    let after_rc = compact_blocks.row_data(0).expect("row 0").filtered_models;
    assert!(
        std::ptr::addr_eq(
            popup_rc.as_any() as *const _,
            after_rc.as_any() as *const _,
        ),
        "the row's filtered_models must be the SAME ModelRc after refilter \
         — otherwise the open popup (bound to the original handle) keeps \
         showing the unfiltered list"
    );

    let names: Vec<String> = (0..popup_rc.row_count())
        .filter_map(|i| popup_rc.row_data(i).map(|m| m.display_name.into()))
        .collect();
    assert_eq!(
        names,
        vec!["Brit Crunch".to_string()],
        "the popup-held Rc must now expose the filtered list"
    );
}
