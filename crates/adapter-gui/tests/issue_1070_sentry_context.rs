//! Issue #1070 — every Sentry event carries the audio setup that was running
//! (interface, rate, buffer, chains, blocks) and the host machine (CPU, RAM).

use adapter_gui::sentry_audio_context::audio_context;
use adapter_gui::sentry_event_context::{attach, publish_audio};
use adapter_gui::sentry_host_context::host_context;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, NamBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;
use sentry::protocol::{Context, Event};
use serde_json::{json, Value};

const DEVICE: &str = "coreaudio:Quantum HD 8:11";

fn endpoint(name: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(DEVICE.into()),
        mode: ChannelMode::Mono,
        channels,
    }
}

fn fixture() -> (Project, Vec<IoBinding>) {
    let binding = IoBinding {
        id: "rig".into(),
        name: "Quantum HD 8".into(),
        inputs: vec![endpoint("Guitar 4", vec![3])],
        outputs: vec![endpoint("Main", vec![0, 1])],
    };
    let chain = Chain {
        id: ChainId("rig:input-4".into()),
        description: Some("DIGITAL".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["rig".into()],
        blocks: vec![AudioBlock {
            id: BlockId("b1".into()),
            enabled: true,
            kind: AudioBlockKind::Nam(NamBlock {
                model: "nam_vox_ac30_a2".into(),
                params: ParameterSet::default(),
            }),
        }],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    };
    let project = Project {
        name: None,
        device_settings: vec![DeviceSettings {
            device_id: DeviceId(DEVICE.into()),
            sample_rate: 44_100,
            buffer_size_frames: 64,
            bit_depth: 32,
            #[cfg(target_os = "linux")]
            realtime: true,
            #[cfg(target_os = "linux")]
            rt_priority: 70,
            #[cfg(target_os = "linux")]
            nperiods: 3,
        }],
        chains: vec![chain],
        midi: None,
    };
    (project, vec![binding])
}

#[test]
fn issue_1070_audio_context_names_interface_rate_buffer_and_chain() {
    let (project, bindings) = fixture();
    let ctx = audio_context(&project, &bindings, Some(44_100));

    assert_eq!(ctx["live_sample_rate"], json!(44_100));
    assert!(ctx["backend"].is_string(), "{ctx:?}");
    let device = &ctx["devices"][0];
    assert_eq!(device["device_id"], json!(DEVICE));
    assert_eq!(device["sample_rate"], json!(44_100));
    assert_eq!(device["buffer_size_frames"], json!(64));

    let chain = &ctx["chains"][0];
    assert_eq!(chain["id"], json!("rig:input-4"));
    assert_eq!(chain["description"], json!("DIGITAL"));
    assert_eq!(chain["enabled"], json!(true));
    assert_eq!(chain["io_bindings"][0]["name"], json!("Quantum HD 8"));
    assert_eq!(chain["io_bindings"][0]["inputs"][0]["channels"], json!([3]));
    assert_eq!(
        chain["io_bindings"][0]["outputs"][0]["device_id"],
        json!(DEVICE)
    );
    assert_eq!(chain["blocks"][0], json!("nam/nam_vox_ac30_a2"));
}

#[test]
fn issue_1070_audio_context_without_runtime_has_no_live_rate() {
    let (project, bindings) = fixture();
    let ctx = audio_context(&project, &bindings, None);
    assert_eq!(ctx["live_sample_rate"], Value::Null);
}

#[test]
fn issue_1070_host_context_reports_cpu_and_memory() {
    let ctx = host_context();
    assert!(ctx["cpu_cores"].as_u64().unwrap_or(0) > 0, "{ctx:?}");
    assert!(ctx["memory_total_mb"].as_u64().unwrap_or(0) > 0, "{ctx:?}");
    assert!(ctx["process_memory_mb"].as_u64().is_some(), "{ctx:?}");
    assert!(ctx["cpu_brand"].is_string(), "{ctx:?}");
}

#[test]
fn issue_1070_every_event_gets_the_published_audio_and_host_contexts() {
    let (project, bindings) = fixture();
    publish_audio(audio_context(&project, &bindings, Some(44_100)));

    let event = attach(Event::default()).expect("event is never dropped");
    let Some(Context::Other(audio)) = event.contexts.get("audio") else {
        panic!("no audio context: {:?}", event.contexts.keys());
    };
    assert_eq!(audio["chains"][0]["id"], json!("rig:input-4"));
    assert!(matches!(
        event.contexts.get("host"),
        Some(Context::Other(_))
    ));
}
