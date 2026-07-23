//! #323 — the looper read model: what every transport (GUI, MCP, gRPC) sees.
//!
//! Merges the PERSISTED parameters (`project::chain::LooperConfig`) with the
//! LIVE transport state published by the audio thread
//! (`engine::LooperStatus`), so no consumer has to join the two itself and no
//! transport gets a different view (the query-parity law).

use engine::{LooperState, LooperStatus};
use project::chain::{Chain, LooperSpeed};
use serde_json::json;

fn state_name(state: LooperState) -> &'static str {
    match state {
        LooperState::Empty => "empty",
        LooperState::Recording => "recording",
        LooperState::Playing => "playing",
        LooperState::Overdubbing => "overdubbing",
        LooperState::Stopped => "stopped",
    }
}

fn speed_name(speed: LooperSpeed) -> &'static str {
    match speed {
        LooperSpeed::Half => "half",
        LooperSpeed::Normal => "normal",
        LooperSpeed::Double => "double",
    }
}

/// JSON view of one chain's loopers. `sample_rate` is the chain's LIVE rate —
/// the frame counts mean nothing without it, and a hardcoded 48000 would lie
/// on a 44.1 kHz interface (#669/#723).
pub fn loopers_json(chain: &Chain, statuses: &[LooperStatus], sample_rate: u32) -> String {
    let rate = f64::from(sample_rate.max(1));
    let loopers: Vec<_> = chain
        .loopers
        .iter()
        .map(|cfg| {
            let live = statuses.iter().find(|s| s.uid == cfg.uid);
            let len_frames = live.map_or(0, |s| s.len_frames);
            json!({
                "uid": cfg.uid,
                "state": state_name(live.map_or(LooperState::Empty, |s| s.state)),
                "position_frames": live.map_or(0, |s| s.position_frames),
                "len_frames": len_frames,
                "length_seconds": len_frames as f64 / rate,
                "layers": live.map_or(0, |s| s.layers),
                "mix": cfg.mix,
                "decay": cfg.decay,
                "speed": speed_name(cfg.speed),
                "reverse": cfg.reverse,
            })
        })
        .collect();

    json!({ "chain": chain.id.0, "sample_rate": sample_rate, "loopers": loopers }).to_string()
}

#[cfg(test)]
#[path = "query_loopers_tests.rs"]
mod tests;
