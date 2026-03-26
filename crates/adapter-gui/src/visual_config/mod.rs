mod brand_bogner;
mod brand_boss;
mod brand_collings;
mod brand_cort;
mod brand_diezel;
mod brand_dumble;
mod brand_emerald;
mod brand_evh;
mod brand_fender;
mod brand_gibson;
mod brand_godin;
mod brand_guild;
mod brand_ibanez;
mod brand_jhs;
mod brand_lakewood;
mod brand_lowden;
mod brand_marshall;
mod brand_martin;
mod brand_mesa;
mod brand_morris;
mod brand_ovation;
mod brand_peavey;
mod brand_rainsong;
mod brand_roland;
mod brand_santa_cruz;
mod brand_suhr;
mod brand_takamine;
mod brand_taylor;
mod brand_vox;
mod brand_yamaha;
mod native_american_clean;
mod native_brit_crunch;
mod native_modern_high_gain;
mod native_blackface_clean;
mod native_tweed_breakup;
mod native_chime;
mod native_american_2x12;
mod native_brit_4x12;
mod native_vintage_1x12;
mod native_analog_warm;
mod native_digital_clean;
mod native_modulated_delay;
mod native_reverse;
mod native_slapback;
mod native_tape_vintage;
mod native_plate_foundation;
mod native_compressor_studio_clean;
mod native_gate_basic;
mod native_eq_three_band_basic;
mod native_cry_classic;
mod native_tremolo_sine;
mod native_octave_simple;
mod native_tuner_chromatic;

/// Visual configuration for block models.
///
/// All colors and fonts live here — never in the business-logic crates.
pub struct ModelVisualConfig {
    pub panel_bg: [u8; 3],
    pub panel_text: [u8; 3],
    pub brand_strip_bg: [u8; 3],
    pub model_font: &'static str,
    pub photo_offset_x: f32,
    pub photo_offset_y: f32,
}

struct VisualConfigEntry {
    brand: &'static str,
    model_id: Option<&'static str>,
    config: ModelVisualConfig,
}

const DEFAULT_CONFIG: ModelVisualConfig = ModelVisualConfig {
    panel_bg: [0x2c, 0x2e, 0x34],
    panel_text: [0x80, 0x90, 0xa0],
    brand_strip_bg: [0x1a, 0x1a, 0x1a],
    model_font: "",
    photo_offset_x: 0.0,
    photo_offset_y: 0.0,
};

fn all_entries() -> Vec<VisualConfigEntry> {
    let mut entries = Vec::new();
    entries.extend(brand_bogner::entries());
    entries.extend(brand_boss::entries());
    entries.extend(brand_collings::entries());
    entries.extend(brand_cort::entries());
    entries.extend(brand_diezel::entries());
    entries.extend(brand_dumble::entries());
    entries.extend(brand_emerald::entries());
    entries.extend(brand_evh::entries());
    entries.extend(brand_fender::entries());
    entries.extend(brand_gibson::entries());
    entries.extend(brand_godin::entries());
    entries.extend(brand_guild::entries());
    entries.extend(brand_ibanez::entries());
    entries.extend(brand_jhs::entries());
    entries.extend(brand_lakewood::entries());
    entries.extend(brand_lowden::entries());
    entries.extend(brand_marshall::entries());
    entries.extend(brand_martin::entries());
    entries.extend(brand_mesa::entries());
    entries.extend(brand_morris::entries());
    entries.extend(brand_ovation::entries());
    entries.extend(brand_peavey::entries());
    entries.extend(brand_rainsong::entries());
    entries.extend(brand_roland::entries());
    entries.extend(brand_santa_cruz::entries());
    entries.extend(brand_suhr::entries());
    entries.extend(brand_takamine::entries());
    entries.extend(brand_taylor::entries());
    entries.extend(brand_vox::entries());
    entries.extend(brand_yamaha::entries());
    entries.push(native_american_clean::entry());
    entries.push(native_brit_crunch::entry());
    entries.push(native_modern_high_gain::entry());
    entries.push(native_blackface_clean::entry());
    entries.push(native_tweed_breakup::entry());
    entries.push(native_chime::entry());
    entries.push(native_american_2x12::entry());
    entries.push(native_brit_4x12::entry());
    entries.push(native_vintage_1x12::entry());
    entries.push(native_analog_warm::entry());
    entries.push(native_digital_clean::entry());
    entries.push(native_modulated_delay::entry());
    entries.push(native_reverse::entry());
    entries.push(native_slapback::entry());
    entries.push(native_tape_vintage::entry());
    entries.push(native_plate_foundation::entry());
    entries.push(native_compressor_studio_clean::entry());
    entries.push(native_gate_basic::entry());
    entries.push(native_eq_three_band_basic::entry());
    entries.push(native_cry_classic::entry());
    entries.push(native_tremolo_sine::entry());
    entries.push(native_octave_simple::entry());
    entries.push(native_tuner_chromatic::entry());
    entries
}

pub fn visual_config_for_model(brand: &str, model_id: &str) -> ModelVisualConfig {
    let entries = all_entries();

    // First try exact match: brand + model_id
    if let Some(entry) = entries
        .iter()
        .find(|e| e.brand == brand && e.model_id == Some(model_id))
    {
        return ModelVisualConfig {
            panel_bg: entry.config.panel_bg,
            panel_text: entry.config.panel_text,
            brand_strip_bg: entry.config.brand_strip_bg,
            model_font: entry.config.model_font,
            photo_offset_x: entry.config.photo_offset_x,
            photo_offset_y: entry.config.photo_offset_y,
        };
    }

    // Then try brand-only match (model_id == None)
    if let Some(entry) = entries
        .iter()
        .find(|e| e.brand == brand && e.model_id.is_none())
    {
        return ModelVisualConfig {
            panel_bg: entry.config.panel_bg,
            panel_text: entry.config.panel_text,
            brand_strip_bg: entry.config.brand_strip_bg,
            model_font: entry.config.model_font,
            photo_offset_x: entry.config.photo_offset_x,
            photo_offset_y: entry.config.photo_offset_y,
        };
    }

    // Then try native model_id match (brand is empty or "native")
    if brand.is_empty() || brand == block_core::BRAND_NATIVE {
        if let Some(entry) = entries
            .iter()
            .find(|e| e.brand.is_empty() && e.model_id == Some(model_id))
        {
            return ModelVisualConfig {
                panel_bg: entry.config.panel_bg,
                panel_text: entry.config.panel_text,
                brand_strip_bg: entry.config.brand_strip_bg,
                model_font: entry.config.model_font,
                photo_offset_x: entry.config.photo_offset_x,
                photo_offset_y: entry.config.photo_offset_y,
            };
        }
    }

    DEFAULT_CONFIG
}
