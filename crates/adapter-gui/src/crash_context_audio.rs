//! Responsibility: describes the running audio setup as a crash-report context.
//!
//! Pure: built on the GUI thread whenever the runtime is (re)built, never on
//! the audio thread.

use domain::io_binding::IoBinding;
use project::chain::Chain;
use project::project::Project;
use serde_json::{json, Value};

pub fn audio_context(
    project: &Project,
    bindings: &[IoBinding],
    live_sample_rate: Option<u32>,
) -> Value {
    let devices: Vec<Value> = project
        .device_settings
        .iter()
        .map(|d| {
            json!({
                "device_id": d.device_id.0,
                "sample_rate": d.sample_rate,
                "buffer_size_frames": d.buffer_size_frames,
                "bit_depth": d.bit_depth,
            })
        })
        .collect();
    let chains: Vec<Value> = project
        .chains
        .iter()
        .map(|c| chain_context(c, bindings))
        .collect();
    json!({
        "backend": infra_cpal::audio_backend_name(),
        "live_sample_rate": live_sample_rate,
        "devices": devices,
        "chains": chains,
    })
}

fn chain_context(chain: &Chain, bindings: &[IoBinding]) -> Value {
    let io_bindings: Vec<Value> = chain
        .io_binding_ids
        .iter()
        .filter_map(|id| bindings.iter().find(|b| &b.id == id))
        .map(|b| serde_json::to_value(b).unwrap_or(Value::Null))
        .collect();
    let blocks: Vec<String> = chain
        .blocks
        .iter()
        .map(|b| b.kind.model_identity().replacen(':', "/", 1))
        .collect();
    json!({
        "id": chain.id.0,
        "description": chain.description,
        "enabled": chain.enabled,
        "io_bindings": io_bindings,
        "blocks": blocks,
    })
}
