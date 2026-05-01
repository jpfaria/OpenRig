//! Golden test: every (effect_type, brand, model_id) the catalog
//! exposes must resolve to byte-identical color values under the new
//! `project::catalog::resolve_color_scheme` and the legacy
//! `crate::visual_config::visual_config_for_model`. Pinned at Phase 4b
//! cutover so deleting the legacy module is provably safe.

#[cfg(test)]
mod tests {
    use crate::visual_config::visual_config_for_model;
    use project::catalog::{resolve_color_scheme, supported_block_models, supported_block_types};

    #[test]
    fn new_resolution_matches_legacy_for_every_known_model() {
        let block_types = supported_block_types();
        assert!(!block_types.is_empty(), "registry came up empty");

        let mut total = 0_usize;
        let mut mismatches: Vec<String> = Vec::new();

        for bt in &block_types {
            let models = match supported_block_models(bt.effect_type) {
                Ok(v) => v,
                Err(_) => continue,
            };
            for item in &models {
                total += 1;
                let new_scheme = resolve_color_scheme(&item.effect_type, &item.brand, &item.model_id);
                let legacy = visual_config_for_model(&item.brand, &item.model_id);

                if new_scheme.panel_bg != legacy.panel_bg {
                    mismatches.push(format!(
                        "panel_bg: {}/{}/{} new={:02x?} legacy={:02x?}",
                        item.effect_type, item.brand, item.model_id, new_scheme.panel_bg, legacy.panel_bg
                    ));
                }
                if new_scheme.panel_text != legacy.panel_text {
                    mismatches.push(format!(
                        "panel_text: {}/{}/{} new={:02x?} legacy={:02x?}",
                        item.effect_type, item.brand, item.model_id, new_scheme.panel_text, legacy.panel_text
                    ));
                }
                if new_scheme.brand_strip_bg != legacy.brand_strip_bg {
                    mismatches.push(format!(
                        "brand_strip_bg: {}/{}/{} new={:02x?} legacy={:02x?}",
                        item.effect_type, item.brand, item.model_id, new_scheme.brand_strip_bg, legacy.brand_strip_bg
                    ));
                }
                if new_scheme.model_font != legacy.model_font {
                    mismatches.push(format!(
                        "model_font: {}/{}/{} new={:?} legacy={:?}",
                        item.effect_type, item.brand, item.model_id, new_scheme.model_font, legacy.model_font
                    ));
                }
                if (new_scheme.photo_offset_x - legacy.photo_offset_x).abs() > f32::EPSILON {
                    mismatches.push(format!(
                        "photo_offset_x: {}/{}/{} new={} legacy={}",
                        item.effect_type, item.brand, item.model_id, new_scheme.photo_offset_x, legacy.photo_offset_x
                    ));
                }
                if (new_scheme.photo_offset_y - legacy.photo_offset_y).abs() > f32::EPSILON {
                    mismatches.push(format!(
                        "photo_offset_y: {}/{}/{} new={} legacy={}",
                        item.effect_type, item.brand, item.model_id, new_scheme.photo_offset_y, legacy.photo_offset_y
                    ));
                }
            }
        }

        assert!(total > 100, "fewer than 100 models exercised — registry didn't load fully? total={total}");

        if !mismatches.is_empty() {
            let preview: Vec<&String> = mismatches.iter().take(20).collect();
            panic!(
                "{} byte mismatch(es) across {} models. First {}:\n{}",
                mismatches.len(),
                total,
                preview.len(),
                preview.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
            );
        }

        eprintln!("Phase 4b golden: {total} models, all byte-identical between legacy and new resolution");
    }
}
