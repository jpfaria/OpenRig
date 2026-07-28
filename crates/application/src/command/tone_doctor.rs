//! #791 — the Tone Doctor as commands, so every transport reaches it.
//!
//! The diagnosis (offline blame-by-ablation over the chain's own signal) and
//! the correction it proposes used to live inside `adapter-gui`; MCP could read
//! the objective quality numbers but never ask for the verdict or apply the
//! fix. These two variants close that gap — MCP derives its tools from this
//! enum, and gRPC will inherit the same pair.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use domain::ids::ChainId;

/// Tone Doctor operations any controller can request.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ToneDoctorCommand {
    /// Diagnose one chain: render its signal, find the symptom and the block
    /// that causes it, and measure a correction that clears it.
    ///
    /// The signal is the chain's own: its selected DI loop when one is loaded,
    /// otherwise the live input the adapter registered. The work is expensive
    /// (the chain is re-rendered once per block), so the result arrives as
    /// `Event::ChainToneDiagnosed` on the async completion channel rather than
    /// as the dispatch return value.
    ///
    /// `genre` picks the calibrated symptom limits (`None` = global defaults);
    /// `seconds` caps how much signal is analysed (`None` = the default window).
    DiagnoseChainTone {
        chain: ChainId,
        genre: Option<String>,
        seconds: Option<u32>,
    },

    /// Apply the fix from that chain's last diagnosis: enable the gating group
    /// when the knob sits in one, then set the measured value. Errors when no
    /// diagnosis has run or when it found nothing to fix.
    ApplyToneDoctorFix { chain: ChainId },
}
