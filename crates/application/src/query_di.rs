//! #829: the per-chain DI loop state as a query.
//!
//! The DI has commands (arm, pick a source, route its output) but its
//! observable state — is it playing, at what level, from which source —
//! only ever reached the chain tile. Read parity means every transport
//! sees the same thing before it decides to arm or swap a loop.

use serde::Serialize;

use crate::di_loader::DiLoopSource;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiLoopReading {
    pub chain: String,
    /// `true` while the dedicated DI stream is playing (#717).
    pub playing: bool,
    /// Playback input peak in dBFS (silent floor when not playing).
    pub in_dbfs: f32,
    /// Playback output peak in dBFS.
    pub out_dbfs: f32,
    /// The loaded loop source (`{"Bundled": "..."}` / `{"File": "..."}`),
    /// `null` when nothing is loaded. Same shape the DI commands take, so a
    /// client can echo it straight back.
    pub source: Option<DiLoopSource>,
}

#[derive(Serialize)]
struct DiPayload<'a> {
    chains: &'a [DiLoopReading],
}

pub fn di_loop_state_json(chains: &[DiLoopReading]) -> String {
    serde_json::to_string(&DiPayload { chains })
        .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

#[cfg(test)]
#[path = "query_di_tests.rs"]
mod tests;
