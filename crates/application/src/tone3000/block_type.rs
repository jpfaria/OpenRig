//! Responsibility: picks the OpenRig block type a TONE3000 tone installs as.

use plugin_loader::manifest::BlockType;

use super::api_enums::Tone3000BlockType;
use super::api_types::Tone;

/// Default block type of a tone, from its format, gear and tags. The user
/// can still override it at install time.
pub fn block_type_for(tone: &Tone) -> Tone3000BlockType {
    if tone.format.as_deref() == Some("ir") {
        let acoustic = tone
            .tags
            .iter()
            .any(|tag| tag.name.to_lowercase().contains("acoustic"));
        return if acoustic {
            Tone3000BlockType::Body
        } else {
            Tone3000BlockType::Cab
        };
    }
    match tone.gear.as_deref() {
        Some("pedal") => Tone3000BlockType::GainPedal,
        Some("outboard") => Tone3000BlockType::Preamp,
        _ => Tone3000BlockType::Amp,
    }
}

impl Tone3000BlockType {
    pub const fn manifest_block_type(self) -> BlockType {
        match self {
            Self::Amp => BlockType::Amp,
            Self::Preamp => BlockType::Preamp,
            Self::GainPedal => BlockType::GainPedal,
            Self::Cab => BlockType::Cab,
            Self::Body => BlockType::Body,
        }
    }
}
