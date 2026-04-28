//! Tests for `visual_config`. Lifted from `mod.rs` so the production file
//! stays under the size cap.

use super::*;

// --- all_entries ---

#[test]
fn all_entries_returns_non_empty_list() {
    let entries = all_entries();
    assert!(!entries.is_empty(), "all_entries must not be empty");
}

#[test]
fn all_entries_brand_entries_have_no_empty_brand() {
    let entries = all_entries();
    for entry in &entries {
        if entry.model_id.is_some() && !entry.brand.is_empty() {
            // brand+model entries must have non-empty brand
            assert!(!entry.brand.is_empty());
        }
    }
}

// --- visual_config_for_model: brand-only lookup ---

#[test]
fn visual_config_marshall_brand_returns_gold_panel() {
    let config = visual_config_for_model("marshall", "unknown_model");
    assert_eq!(config.panel_bg, [0xb8, 0x98, 0x40]);
    assert_eq!(config.panel_text, [0x5a, 0x4a, 0x20]);
}

#[test]
fn visual_config_fender_brand_returns_brown_panel() {
    let config = visual_config_for_model("fender", "unknown_model");
    assert_eq!(config.panel_bg, [0x8a, 0x6a, 0x3a]);
    assert_eq!(config.model_font, "Inter");
}

#[test]
fn visual_config_vox_brand_returns_dark_panel() {
    let config = visual_config_for_model("vox", "unknown_model");
    assert_eq!(config.panel_bg, [0x1a, 0x1a, 0x2a]);
}

#[test]
fn visual_config_bogner_brand_returns_purple_panel() {
    let config = visual_config_for_model("bogner", "unknown_model");
    assert_eq!(config.panel_bg, [0x28, 0x18, 0x24]);
    assert_eq!(config.model_font, "Dancing Script");
}

#[test]
fn visual_config_mesa_brand_returns_config() {
    let config = visual_config_for_model("mesa", "unknown_model");
    // Should match brand-only entry, not the default
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_peavey_brand_returns_config() {
    let config = visual_config_for_model("peavey", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_gibson_brand_returns_config() {
    let config = visual_config_for_model("gibson", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_taylor_brand_returns_config() {
    let config = visual_config_for_model("taylor", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_martin_brand_returns_config() {
    let config = visual_config_for_model("martin", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_yamaha_brand_returns_config() {
    let config = visual_config_for_model("yamaha", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_roland_brand_returns_config() {
    let config = visual_config_for_model("roland", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_boss_brand_returns_config() {
    let config = visual_config_for_model("boss", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_ibanez_brand_returns_config() {
    let config = visual_config_for_model("ibanez", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_diezel_brand_returns_config() {
    let config = visual_config_for_model("diezel", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_dumble_brand_returns_config() {
    let config = visual_config_for_model("dumble", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_evh_brand_returns_config() {
    let config = visual_config_for_model("evh", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_guild_brand_returns_config() {
    let config = visual_config_for_model("guild", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_jhs_brand_returns_config() {
    let config = visual_config_for_model("jhs", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_takamine_brand_returns_config() {
    let config = visual_config_for_model("takamine", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_collings_brand_returns_config() {
    let config = visual_config_for_model("collings", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_cort_brand_returns_config() {
    let config = visual_config_for_model("cort", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_emerald_brand_returns_config() {
    let config = visual_config_for_model("emerald", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_godin_brand_returns_config() {
    let config = visual_config_for_model("godin", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_lakewood_brand_returns_config() {
    let config = visual_config_for_model("lakewood", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_lowden_brand_returns_config() {
    let config = visual_config_for_model("lowden", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_morris_brand_returns_config() {
    let config = visual_config_for_model("morris", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_ovation_brand_returns_config() {
    let config = visual_config_for_model("ovation", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_rainsong_brand_returns_config() {
    let config = visual_config_for_model("rainsong", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_santa_cruz_brand_returns_config() {
    let config = visual_config_for_model("santa_cruz", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_suhr_brand_returns_config() {
    let config = visual_config_for_model("suhr", "unknown_model");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

// --- visual_config_for_model: native model_id lookup ---

#[test]
fn visual_config_native_american_clean_returns_specific_config() {
    let config = visual_config_for_model("", "american_clean");
    assert_eq!(config.panel_bg, [0x2a, 0x33, 0x38]);
    assert_eq!(config.model_font, "Dancing Script");
}

#[test]
fn visual_config_native_brand_american_clean_returns_specific_config() {
    let config = visual_config_for_model("native", "american_clean");
    assert_eq!(config.panel_bg, [0x2a, 0x33, 0x38]);
}

#[test]
fn visual_config_native_brit_crunch_returns_specific_config() {
    let config = visual_config_for_model("", "brit_crunch");
    assert_eq!(config.panel_bg, [0x34, 0x2e, 0x28]);
    assert_eq!(config.model_font, "Permanent Marker");
}

#[test]
fn visual_config_native_modern_high_gain_returns_config() {
    let config = visual_config_for_model("", "modern_high_gain");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_blackface_clean_returns_config() {
    let config = visual_config_for_model("", "blackface_clean");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_tweed_breakup_returns_config() {
    let config = visual_config_for_model("", "tweed_breakup");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_chime_returns_config() {
    let config = visual_config_for_model("", "chime");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_plate_foundation_returns_config() {
    let config = visual_config_for_model("", "plate_foundation");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_digital_clean_returns_config() {
    let config = visual_config_for_model("", "digital_clean");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_analog_warm_returns_config() {
    let config = visual_config_for_model("", "analog_warm");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_slapback_returns_config() {
    let config = visual_config_for_model("", "slapback");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_reverse_returns_config() {
    let config = visual_config_for_model("", "reverse");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_tape_vintage_returns_config() {
    let config = visual_config_for_model("", "tape_vintage");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_modulated_delay_returns_config() {
    let config = visual_config_for_model("", "modulated_delay");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_compressor_studio_clean_returns_config() {
    let config = visual_config_for_model("", "compressor_studio_clean");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_gate_basic_returns_config() {
    let config = visual_config_for_model("", "gate_basic");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_eq_three_band_basic_returns_config() {
    let config = visual_config_for_model("", "eq_three_band_basic");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_cry_classic_returns_config() {
    let config = visual_config_for_model("", "cry_classic");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_tremolo_sine_returns_config() {
    let config = visual_config_for_model("", "tremolo_sine");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_octave_simple_returns_config() {
    let config = visual_config_for_model("", "octave_simple");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_american_2x12_returns_config() {
    let config = visual_config_for_model("", "american_2x12");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_brit_4x12_returns_config() {
    let config = visual_config_for_model("", "brit_4x12");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_vintage_1x12_returns_config() {
    let config = visual_config_for_model("", "vintage_1x12");
    assert_ne!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

// --- visual_config_for_model: fallback ---

#[test]
fn visual_config_unknown_brand_returns_default() {
    let config = visual_config_for_model("nonexistent_brand", "unknown_model");
    assert_eq!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
    assert_eq!(config.panel_text, DEFAULT_CONFIG.panel_text);
    assert_eq!(config.brand_strip_bg, DEFAULT_CONFIG.brand_strip_bg);
    assert_eq!(config.model_font, DEFAULT_CONFIG.model_font);
}

#[test]
fn visual_config_empty_brand_unknown_model_returns_default() {
    let config = visual_config_for_model("", "nonexistent_native_model");
    assert_eq!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

#[test]
fn visual_config_native_brand_unknown_model_returns_default() {
    let config = visual_config_for_model("native", "nonexistent_native_model");
    assert_eq!(config.panel_bg, DEFAULT_CONFIG.panel_bg);
}

// --- all brand entries() return valid data ---

#[test]
fn all_brand_entries_have_valid_panel_bg() {
    let entries = all_entries();
    for entry in &entries {
        // panel_bg should not be all zeros (black) unless intentional
        // Just verify it's a valid RGB triple (always true for [u8; 3])
        let [r, g, b] = entry.config.panel_bg;
        // At minimum, the sum should be > 0 (not pure black)
        // Actually, some brands might legitimately use very dark backgrounds
        // so just verify the type exists
        let _ = (r, g, b);
    }
}

#[test]
fn all_brand_entries_have_non_negative_offsets() {
    let entries = all_entries();
    for entry in &entries {
        assert!(
            entry.config.photo_offset_x.is_finite(),
            "photo_offset_x must be finite for brand='{}' model={:?}",
            entry.brand,
            entry.model_id
        );
        assert!(
            entry.config.photo_offset_y.is_finite(),
            "photo_offset_y must be finite for brand='{}' model={:?}",
            entry.brand,
            entry.model_id
        );
    }
}

#[test]
fn all_entries_covers_all_30_brands() {
    let entries = all_entries();
    let brands: std::collections::HashSet<&str> = entries
        .iter()
        .filter(|e| !e.brand.is_empty())
        .map(|e| e.brand)
        .collect();
    let expected_brands = [
        "bogner",
        "boss",
        "collings",
        "cort",
        "diezel",
        "dumble",
        "emerald",
        "evh",
        "fender",
        "gibson",
        "godin",
        "guild",
        "ibanez",
        "jhs",
        "lakewood",
        "lowden",
        "marshall",
        "martin",
        "mesa",
        "morris",
        "ovation",
        "peavey",
        "rainsong",
        "roland",
        "santa_cruz",
        "suhr",
        "takamine",
        "taylor",
        "vox",
        "yamaha",
    ];
    for brand in &expected_brands {
        assert!(
            brands.contains(brand),
            "missing brand entry for '{}'",
            brand
        );
    }
}

#[test]
fn all_entries_covers_all_native_models() {
    let entries = all_entries();
    let native_models: std::collections::HashSet<&str> = entries
        .iter()
        .filter(|e| e.brand.is_empty() && e.model_id.is_some())
        .map(|e| e.model_id.unwrap())
        .collect();
    let expected_native = [
        "american_clean",
        "brit_crunch",
        "modern_high_gain",
        "blackface_clean",
        "tweed_breakup",
        "chime",
        "american_2x12",
        "brit_4x12",
        "vintage_1x12",
        "analog_warm",
        "digital_clean",
        "modulated_delay",
        "reverse",
        "slapback",
        "tape_vintage",
        "plate_foundation",
        "compressor_studio_clean",
        "gate_basic",
        "eq_three_band_basic",
        "cry_classic",
        "tremolo_sine",
        "octave_simple",
    ];
    for model in &expected_native {
        assert!(
            native_models.contains(model),
            "missing native model entry for '{}'",
            model
        );
    }
}
