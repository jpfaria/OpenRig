//! #913 — the plugin info overlay's payload.
//!
//! The overlay is the only place a user reads what a block IS, so it has to
//! answer for every model — including one with no metadata file, which is the
//! common case for a freshly dropped capture. `has_homepage` is what gates the
//! button: true with an empty URL would offer a link that opens nothing.

use super::build_plugin_info;

fn asset_paths_ready() {
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
}

#[test]
fn a_model_with_no_metadata_still_produces_a_panel() {
    asset_paths_ready();
    let (data, homepage) = build_plugin_info("gain", "no_such_model_913", "en-US");
    assert!(data.description.is_empty());
    assert!(data.license.is_empty());
    assert!(homepage.is_empty());
}

#[test]
fn an_empty_homepage_does_not_offer_the_button() {
    asset_paths_ready();
    let (data, _) = build_plugin_info("gain", "no_such_model_913", "en-US");
    assert!(
        !data.has_homepage,
        "a link that opens nothing must not be offered"
    );
    assert!(data.homepage.is_empty());
}

#[test]
fn the_homepage_is_returned_alongside_the_panel_for_the_button_to_keep() {
    asset_paths_ready();
    let (data, homepage) = build_plugin_info("gain", "no_such_model_913", "en-US");
    assert_eq!(
        homepage,
        data.homepage.to_string(),
        "the button and the panel must not drift apart"
    );
}

#[test]
fn a_model_with_no_screenshot_says_so_instead_of_showing_a_blank() {
    asset_paths_ready();
    let (data, _) = build_plugin_info("gain", "no_such_model_913", "en-US");
    assert!(
        !data.has_screenshot,
        "the caller draws the placeholder when this is false"
    );
}

#[test]
fn the_catalog_fields_are_filled_from_the_catalog_not_left_empty() {
    asset_paths_ready();
    // Whatever the catalog answers for an unknown model, the panel must carry
    // the catalog's answer rather than inventing one.
    let (data, _) = build_plugin_info("gain", "no_such_model_913", "en-US");
    assert_eq!(
        data.plugin_name.to_string(),
        project::catalog::model_display_name("gain", "no_such_model_913")
    );
    assert_eq!(
        data.brand.to_string(),
        project::catalog::model_brand("gain", "no_such_model_913")
    );
    assert_eq!(
        data.type_label.to_string(),
        project::catalog::model_type_label("gain", "no_such_model_913")
    );
}

#[test]
fn an_unknown_language_falls_back_without_panicking() {
    asset_paths_ready();
    let (data, _) = build_plugin_info("gain", "no_such_model_913", "xx-YY");
    assert!(data.description.is_empty());
}
