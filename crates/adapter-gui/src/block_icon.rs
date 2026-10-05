//! Responsibility: picks the icon a block kind is drawn with.

/// Returns the accent color (RGBA) for an effect type icon_kind.
/// Single source of truth — used by all UI components.
pub fn accent_color_for_icon_kind(icon_kind: &str) -> slint::Color {
    // #398: the category palette the redesign draws blocks with.
    let rgb = |v: u32| slint::Color::from_argb_u8(255, (v >> 16) as u8, (v >> 8) as u8, v as u8);
    match icon_kind {
        "preamp" => rgb(0xb8862a),
        "amp" | "nam" => rgb(0xd23b2d),
        "full_rig" => rgb(0xb5452f),
        "cab" => rgb(0x7c6d5a),
        "body" => rgb(0x8f6a45),
        "ir" => rgb(0x6f6a62),
        "gain" => rgb(0xe8671b),
        "dynamics" => rgb(0x2f6fe0),
        "filter" => rgb(0xc9a100),
        "wah" => rgb(0xa88f12),
        "modulation" => rgb(0x8a4fe0),
        "delay" => rgb(0x0f9c8e),
        "reverb" => rgb(0x2b8fd6),
        "pitch" => rgb(0x6a5ae0),
        "utility" => rgb(0x69717f),
        "insert" => rgb(0xf28c1e),
        "input" | "output" => rgb(0x2b8fd6),
        _ => slint::Color::from_argb_u8(255, 0x7f, 0xb0, 0xff),
    }
}

/// Icon SVG index for an icon_kind. Used to load the correct icon from a pre-built array.
/// Returns a numeric index into EFFECT_TYPE_ICONS.
#[allow(dead_code)]
pub fn icon_index_for_icon_kind(icon_kind: &str) -> usize {
    match icon_kind {
        "preamp" => 0,
        "amp" => 1,
        "cab" => 2,
        "body" => 3,
        "ir" => 4,
        "full_rig" => 5,
        "gain" => 6,
        "dynamics" => 7,
        "filter" => 8,
        "wah" => 9,
        "modulation" => 10,
        "delay" => 11,
        "reverb" => 12,
        "utility" => 13,
        "nam" => 14,
        "pitch" => 15,
        _ => 13, // utility fallback
    }
}

pub fn block_family_for_kind(kind: &str) -> &'static str {
    use block_core::*;
    match kind {
        EFFECT_TYPE_PREAMP | EFFECT_TYPE_AMP | EFFECT_TYPE_FULL_RIG | EFFECT_TYPE_NAM => "amp",
        EFFECT_TYPE_CAB => "cab",
        EFFECT_TYPE_BODY => "body",
        EFFECT_TYPE_IR => "ir",
        EFFECT_TYPE_GAIN => "gain",
        EFFECT_TYPE_DYNAMICS => "dynamics",
        EFFECT_TYPE_FILTER => "filter",
        EFFECT_TYPE_WAH => "wah",
        EFFECT_TYPE_PITCH => "pitch",
        EFFECT_TYPE_MODULATION => "modulation",
        EFFECT_TYPE_DELAY | EFFECT_TYPE_REVERB => "space",
        EFFECT_TYPE_UTILITY => "utility",
        "input" | "output" | "insert" => "routing",
        _ => "utility",
    }
}

// ── I/O binding Slint bridge (#716) ──────────────────────────────────────────
