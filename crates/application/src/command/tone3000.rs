//! Responsibility: names the TONE3000 browser commands.
//!
//! The user's own Secret Key reaches TONE3000; a search and an install run
//! off the dispatching thread and report back as events. `Debug` never
//! prints the key, so a logged command cannot leak it (#879).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::plugin_library::EditorGrid;
use crate::tone3000::{
    Tone3000Architecture, Tone3000BlockType, Tone3000Format, Tone3000Gear, Tone3000Sort,
};

/// Every TONE3000 browser state change any controller can request.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub enum Tone3000Command {
    /// Search the public TONE3000 catalog. `page` starts at 1.
    SearchTone3000 {
        query: String,
        page: u32,
        format: Option<Tone3000Format>,
        gear: Option<Tone3000Gear>,
        sort: Option<Tone3000Sort>,
    },

    /// Download every capture of one tone and install it as a plugin.
    /// `None` picks A2 when the tone has A2 captures, and the block type
    /// its gear suggests.
    InstallTone3000 {
        tone_id: u64,
        architecture: Option<Tone3000Architecture>,
        block_type: Option<Tone3000BlockType>,
    },

    /// Install a tone that waits for names, with the parameters `grid`
    /// gives its captures (one row per capture of the waiting grid).
    FinishTone3000Install { tone_id: u64, grid: EditorGrid },

    /// Drop a tone that waits for names; nothing of it stays on disk.
    CancelTone3000Install { tone_id: u64 },

    /// The user's TONE3000 Secret Key (`t3k_cs_…`). Blank clears it.
    SetTone3000ApiKey { key: String },

    /// Remove a TONE3000 plugin from the plugins folder.
    UninstallTone3000 { plugin_id: String },

    /// Download an installed tone again, replacing its package in place.
    /// `None` takes A2 when the tone has A2 captures.
    UpdateTone3000 {
        tone_id: u64,
        architecture: Option<Tone3000Architecture>,
    },
}

impl std::fmt::Debug for Tone3000Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SearchTone3000 {
                query,
                page,
                format,
                gear,
                sort,
            } => f
                .debug_struct("SearchTone3000")
                .field("query", query)
                .field("page", page)
                .field("format", format)
                .field("gear", gear)
                .field("sort", sort)
                .finish(),
            Self::InstallTone3000 {
                tone_id,
                architecture,
                block_type,
            } => f
                .debug_struct("InstallTone3000")
                .field("tone_id", tone_id)
                .field("architecture", architecture)
                .field("block_type", block_type)
                .finish(),
            Self::FinishTone3000Install { tone_id, grid } => f
                .debug_struct("FinishTone3000Install")
                .field("tone_id", tone_id)
                .field("grid", grid)
                .finish(),
            Self::CancelTone3000Install { tone_id } => f
                .debug_struct("CancelTone3000Install")
                .field("tone_id", tone_id)
                .finish(),
            Self::SetTone3000ApiKey { .. } => f
                .debug_struct("SetTone3000ApiKey")
                .field("key", &"<redacted>")
                .finish(),
            Self::UninstallTone3000 { plugin_id } => f
                .debug_struct("UninstallTone3000")
                .field("plugin_id", plugin_id)
                .finish(),
            Self::UpdateTone3000 {
                tone_id,
                architecture,
            } => f
                .debug_struct("UpdateTone3000")
                .field("tone_id", tone_id)
                .field("architecture", architecture)
                .finish(),
        }
    }
}
