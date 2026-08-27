//! Responsibility: builds what the plugin info overlay shows for one model.
//!
//! Split out of `plugin_info_inline_wiring` (#913). Flipping the overlay
//! visible is screen work; assembling the panel — catalog name, brand, type
//! label, the localized metadata and the screenshot — is not, and it has to
//! answer for a model nobody has metadata for as readily as for one that does.

use crate::plugin_info;
use crate::project_view::load_screenshot_image;
use crate::PluginInfoData;
use project::catalog::{model_brand, model_display_name, model_type_label};

/// Assemble the overlay's payload for `model_id`, plus the homepage the
/// "open homepage" button will use.
///
/// The homepage is returned separately because the button fires long after the
/// panel was built, so the wiring keeps it rather than reading it back off a
/// Slint property.
pub(crate) fn build_plugin_info(
    effect_type: &str,
    model_id: &str,
    lang: &str,
) -> (PluginInfoData, String) {
    let meta = plugin_info::plugin_metadata(lang, model_id);
    let (screenshot, has_screenshot) = load_screenshot_image(effect_type, model_id);
    let homepage = meta.homepage.clone();
    let data = PluginInfoData {
        screenshot,
        has_screenshot,
        plugin_name: model_display_name(effect_type, model_id).into(),
        brand: model_brand(effect_type, model_id).into(),
        type_label: model_type_label(effect_type, model_id).into(),
        description: meta.description.into(),
        license: meta.license.into(),
        homepage: homepage.clone().into(),
        has_homepage: !homepage.is_empty(),
    };
    (data, homepage)
}

#[cfg(test)]
#[path = "plugin_info_panel_tests.rs"]
mod tests;
