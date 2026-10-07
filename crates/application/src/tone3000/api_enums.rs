//! Responsibility: names the TONE3000 query options the app exposes.
//!
//! Every enum serializes to the spelling the API uses, so a command coming
//! from MCP reads the same as the query string it becomes (#879).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Capture file format of a tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Tone3000Format {
    Nam,
    Ir,
}

impl Tone3000Format {
    pub const fn as_api_str(self) -> &'static str {
        match self {
            Self::Nam => "nam",
            Self::Ir => "ir",
        }
    }
}

/// Gear category a tone was captured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Tone3000Gear {
    Amp,
    FullRig,
    AmpCab,
    Pedal,
    Outboard,
    Ir,
    Cab,
}

impl Tone3000Gear {
    pub const fn as_api_str(self) -> &'static str {
        match self {
            Self::Amp => "amp",
            Self::FullRig => "full-rig",
            Self::AmpCab => "amp-cab",
            Self::Pedal => "pedal",
            Self::Outboard => "outboard",
            Self::Ir => "ir",
            Self::Cab => "cab",
        }
    }
}

/// Order of a search page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Tone3000Sort {
    BestMatch,
    Newest,
    Oldest,
    Trending,
    DownloadsAllTime,
}

impl Tone3000Sort {
    pub const fn as_api_str(self) -> &'static str {
        match self {
            Self::BestMatch => "best-match",
            Self::Newest => "newest",
            Self::Oldest => "oldest",
            Self::Trending => "trending",
            Self::DownloadsAllTime => "downloads-all-time",
        }
    }
}

/// NAM architecture family to download: A1 (WaveNet / LSTM) or A2
/// (`SlimmableContainer`). Spelled like the manifest's `architecture`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Tone3000Architecture {
    A1,
    A2,
}

impl Tone3000Architecture {
    /// Value of the `architecture` filter of `GET /models`.
    pub const fn as_api_str(self) -> &'static str {
        match self {
            Self::A1 => "1",
            Self::A2 => "2",
        }
    }

    /// Short tag used in plugin ids (`tone3000_<id>_a2`).
    pub const fn as_id_suffix(self) -> &'static str {
        match self {
            Self::A1 => "a1",
            Self::A2 => "a2",
        }
    }
}

/// Block an installed tone becomes. Spelled like the manifest's `type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Tone3000BlockType {
    Amp,
    Preamp,
    GainPedal,
    Cab,
    Body,
}
