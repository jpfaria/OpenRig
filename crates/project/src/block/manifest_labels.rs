//! Sanitisation of human-facing labels coming from external plugin
//! manifests (NAM/IR `GridParameter`, LV2 port names, VST3 parameter
//! display names) before they reach the Slint UI.
//!
//! The OpenRig UI ships a curated set of fonts (Bebas Neue, Inter,
//! Permanent Marker, Orbitron, Cooper Hewitt, Dancing Script, Ek Mukta)
//! plus locale-aware system faces for CJK / Devanagari. None of them
//! carry emoji / pictograph glyphs. macOS' Slint backend cascades to
//! Apple Color Emoji automatically; Windows / Linux do not, and emojis
//! render as black tofu boxes (issue #424 — Bogner Ecstasy capture
//! labels like `"✋ 4X12"` showing up as `"□ 4X12"`).
//!
//! Project policy (`feedback_no_font_glyphs_for_icons`): never use a
//! glyph as an icon — always SVG. We can't rewrite third-party manifests
//! we don't own, so we strip the offending codepoints at the seam where
//! manifest data turns into `ParameterSpec`. The raw value strings used
//! for capture lookup / persistence stay untouched, only the *displayed*
//! label is cleaned.
//!
//! Stripped ranges are deliberately conservative — symbols / pictograph
//! blocks that no shipped font carries — so Latin, Cyrillic, Greek,
//! Hebrew, Arabic, Devanagari and CJK keep working untouched.

/// Remove emoji / pictograph codepoints from `input` and trim the
/// result. Returns the original (untrimmed) string when stripping
/// would leave nothing to display, so a label that was *only* an
/// emoji at least falls back to its raw form rather than vanishing.
pub fn sanitize_label(input: &str) -> String {
    let cleaned: String = input.chars().filter(|c| !is_pictograph(*c)).collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        input.to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_pictograph(c: char) -> bool {
    let n = c as u32;
    // Misc Symbols + Dingbats (✋ U+270B, ★ U+2605, ☎ U+260E, …).
    (0x2300..=0x27BF).contains(&n)
        // Supplemental Arrows-B + Misc Mathematical Symbols-B.
        || (0x2900..=0x29FF).contains(&n)
        // Misc Symbols and Arrows.
        || (0x2B00..=0x2BFF).contains(&n)
        // Plane 1 pictographs (emoji, transport, supplemental symbols,
        // CJK-style ideographic *symbols* — not the unified ideographs).
        || (0x1F000..=0x1FFFF).contains(&n)
        // Variation selectors (VS-1..16) used to switch to emoji
        // presentation; orphaned after stripping the base codepoint.
        || (0xFE00..=0xFE0F).contains(&n)
        // Zero-width joiner used to compose multi-codepoint emoji.
        || n == 0x200D
}

#[cfg(test)]
#[path = "manifest_labels_tests.rs"]
mod tests;
