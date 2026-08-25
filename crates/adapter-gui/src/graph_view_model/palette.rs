//! Responsibility: gives each node category its colour
//!
//! The component is colour-agnostic; the host supplies a palette or takes
//! [`default_palette`]. Single source of truth — the default lives here only.

use super::types::NodeCategory;

/// Visual style for a node category. Colours are `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CategoryStyle {
    pub category: &'static str,
    pub fill: u32,
    pub border: u32,
}

fn darken(rgb: u32, factor: f32) -> u32 {
    let r = ((rgb >> 16) & 0xff) as f32 * (1.0 - factor);
    let g = ((rgb >> 8) & 0xff) as f32 * (1.0 - factor);
    let b = (rgb & 0xff) as f32 * (1.0 - factor);
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Default palette covering every [`NodeCategory`]. Host may override
/// fully; this is just sane defaults.
pub fn default_palette() -> Vec<CategoryStyle> {
    [
        (NodeCategory::Input, 0x1e8aff),
        (NodeCategory::Output, 0xff8a1e),
        (NodeCategory::Dynamics, 0xc69b3f),
        (NodeCategory::Drive, 0xd04a3a),
        (NodeCategory::Amp, 0x4a7fd0),
        (NodeCategory::Modulation, 0x8d4ad0),
        (NodeCategory::Time, 0x2d9b9b),
        (NodeCategory::Reverb, 0x3aa86a),
        (NodeCategory::Eq, 0xc8b400),
        (NodeCategory::Util, 0x6a7483),
        (NodeCategory::Other, 0x8a94a2),
    ]
    .into_iter()
    .map(|(cat, fill)| CategoryStyle {
        category: cat.as_str(),
        fill,
        border: darken(fill, 0.35),
    })
    .collect()
}
