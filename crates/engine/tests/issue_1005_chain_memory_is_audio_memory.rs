//! A chain's runtime — its blocks' delay lines and its route buffers — is
//! built in the audio zone, so wiring only that zone keeps everything the DSP
//! touches resident. macOS only, like the wiring.
#![cfg(target_os = "macos")]

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use engine::audio_zone_regions::audio_zone_bytes_in_use;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

fn registry() -> Vec<IoBinding> {
    let endpoint = |mode, channels: Vec<usize>| IoEndpoint {
        name: "ep".into(),
        device_id: DeviceId("dev".into()),
        mode,
        channels,
    };
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![endpoint(ChannelMode::Mono, vec![0])],
        outputs: vec![endpoint(ChannelMode::Stereo, vec![0, 1])],
    }]
}

fn delay(id: &str) -> AudioBlock {
    let mut params = ParameterSet::default();
    for (name, value) in [
        ("time_ms", 500.0),
        ("feedback", 30.0),
        ("mix", 50.0),
        ("tone", 50.0),
    ] {
        params.insert(name, ParameterValue::Float(value));
    }
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model: "analog_warm".into(),
            params,
        }),
    }
}

fn chain_with_delays() -> Chain {
    Chain {
        id: ChainId("chain:delays".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: (0..4).map(|i| delay(&format!("delay-{i}"))).collect(),
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

#[test]
fn a_chains_delay_lines_are_built_in_the_audio_zone() {
    block_delay::register_natives();
    assert!(engine::audio_zone_router::install());
    let before = audio_zone_bytes_in_use();
    let runtime = engine::runtime::build_chain_runtime_state(
        &chain_with_delays(),
        48_000.0,
        &[],
        &registry(),
    )
    .expect("the chain builds");
    let grown = audio_zone_bytes_in_use().saturating_sub(before);
    drop(runtime);
    assert!(
        grown >= 1 << 20,
        "four delays hold megabytes of delay line; the audio zone grew only \
         {grown} bytes, so the runtime was built outside it and would not be \
         wired"
    );
}
