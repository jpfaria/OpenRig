//! Issue #613 — three bugs on the compact chain view (Chains screen).
//!
//! These guards scan the compact view's `.slint` source (same approach
//! the i18n tests use) and pin the expected end state:
//!
//! 1. The measured latency must be shown INSIDE the compact view (badge),
//!    not only on the main chains list behind the overlay. The compact
//!    page therefore exposes a `latency-ms` property and renders the
//!    same `label-lat` badge the main row uses.
//! 2. The compact view is single-chain focused: it must NOT carry the
//!    move-chain-up / move-chain-down reorder controls.
//! 3. The compact view's block-type picker must use the same `BlockTypeCard`
//!    component as the main chains screen (single source of truth for the
//!    tile icons), not a separate `EffectTypeIcon` rendering — otherwise
//!    VST3 / input / output / insert show the wrong icons.

use std::path::PathBuf;

fn compact_view_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui/pages/compact_chain_view.slint");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// #761: the header cluster (chain title, volume, DI loop, latency badge,
/// gear, trash) was extracted out of `compact_chain_view.slint` into its
/// own file to keep the page under the 500-line cap. The latency-badge
/// assertion below cares whether the compact view's RENDERED page includes
/// the badge, not which specific file the markup lives in — so it scans
/// both.
fn compact_view_header_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui/pages/compact_chain_view_header.slint");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn compact_view_has_no_chain_reorder_buttons() {
    let src = compact_view_source();
    assert!(
        !src.contains("move-chain-up"),
        "compact_chain_view.slint must not reference move-chain-up \
         (chain reorder has no place in the single-chain compact view)"
    );
    assert!(
        !src.contains("move-chain-down"),
        "compact_chain_view.slint must not reference move-chain-down \
         (chain reorder has no place in the single-chain compact view)"
    );
}

#[test]
fn compact_view_type_picker_uses_block_type_card() {
    let src = compact_view_source();
    assert!(
        src.contains("BlockTypeCard"),
        "compact_chain_view.slint must render type tiles with BlockTypeCard \
         (same component as the main chains screen) so the icons match"
    );
    assert!(
        !src.contains("EffectTypeIcon"),
        "compact_chain_view.slint must not render the type picker with \
         EffectTypeIcon — that produces a different icon set than the main \
         screen for VST3 / input / output / insert"
    );
}

#[test]
fn compact_view_renders_latency_badge() {
    let src = compact_view_source();
    assert!(
        src.contains("latency-ms"),
        "compact_chain_view.slint must expose a latency-ms property so the \
         measured latency renders inside the compact view"
    );
    let src_and_header = format!("{src}\n{}", compact_view_header_source());
    assert!(
        src_and_header.contains("label-lat"),
        "the compact view's page (compact_chain_view.slint) or its header \
         (compact_chain_view_header.slint) must render the label-lat \
         latency badge (parity with the main chain row)"
    );
}
