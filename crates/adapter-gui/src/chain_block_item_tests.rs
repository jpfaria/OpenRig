//! #328 (spec §5.4) — the split is one chip in the strip and the compact view.

use super::chain_block_item_from_block;
use crate::chain_graph_fixtures_tests::{core, split};
use project::block::SplitEnd;

#[test]
fn a_split_is_one_split_chip() {
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let chip = chain_block_item_from_block(&split("sp", SplitEnd::Mix, vec![core("a1")], vec![]));
    assert_eq!(
        (
            chip.kind.as_str(),
            chip.icon_kind.as_str(),
            chip.type_label.as_str()
        ),
        ("split", "split", "SPLIT")
    );
    assert_eq!(
        chip.label.to_string(),
        rust_i18n::t!("picker-split-mix").to_string()
    );
    assert_eq!(
        chip.display_name.as_str(),
        "",
        "no model tooltip on a split"
    );
}

#[test]
fn a_y_split_says_so() {
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let chip = chain_block_item_from_block(&split("sp", SplitEnd::Y, vec![], vec![]));
    assert_eq!(
        chip.label.to_string(),
        rust_i18n::t!("picker-split-y").to_string()
    );
}
