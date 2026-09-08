//! #923 — with an enabled Insert, EVERY tail output must receive the final
//! segment, exactly as it does with the inserts bypassed.
//!
//! The owner's rig: one chain selecting two bindings that read the same
//! interface input (Main out `[0,1]` and ADAT 3/4 out `[16,17]`), plus two
//! inserts whose loops live on that same interface. Only Main played; the
//! second tail output stayed silent. These drive the production
//! `build_per_input_runtimes` path with real buffers and close both loops in
//! software (send → return on the next callback).

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use super::{process_input_f32, process_output_f32};
use crate::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use crate::runtime_graph::build_per_input_runtimes;

const QUANTUM: &str = "coreaudio:quantum";
const DEVICE_CHANNELS: usize = 26;
const FRAMES: usize = 128;

const GUITAR_IN: usize = 0;
const MAIN_OUT: [usize; 2] = [0, 1];
const ADAT_OUT: [usize; 2] = [16, 17];
const PEDAIS_SEND: usize = 11;
const PEDAIS_RETURN: usize = 25;
const SYN2_SEND: usize = 10;
const SYN2_RETURN: [usize; 2] = [17, 18];

fn endpoint(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(QUANTUM.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "guitarra-1".into(),
            name: "Guitarra 1".into(),
            inputs: vec![endpoint("in", ChannelMode::Mono, &[GUITAR_IN])],
            outputs: vec![endpoint("main", ChannelMode::Stereo, &MAIN_OUT)],
        },
        IoBinding {
            id: "guitarra-1-5050".into(),
            name: "Guitarra 1 50/50".into(),
            inputs: vec![endpoint("in", ChannelMode::Mono, &[GUITAR_IN])],
            outputs: vec![endpoint("adat34", ChannelMode::Stereo, &ADAT_OUT)],
        },
        IoBinding {
            id: "pedais".into(),
            name: "Pedais".into(),
            inputs: vec![endpoint("ret", ChannelMode::Mono, &[PEDAIS_RETURN])],
            outputs: vec![endpoint("snd", ChannelMode::Mono, &[PEDAIS_SEND])],
        },
        IoBinding {
            id: "syn2-main".into(),
            name: "SYN2".into(),
            inputs: vec![endpoint("ret", ChannelMode::Stereo, &SYN2_RETURN)],
            outputs: vec![endpoint("snd", ChannelMode::Mono, &[SYN2_SEND])],
        },
    ]
}

fn gain(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn insert(id: &str, io: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "external_loop".into(),
            io: io.into(),
        }),
    }
}

/// `drv → insert(pedais) → insert(syn2) → amp`, head/tail from the bindings.
fn owner_chain(io_binding_ids: &[&str]) -> Chain {
    Chain {
        id: ChainId("rig:input-2".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: io_binding_ids.iter().map(|s| s.to_string()).collect(),
        blocks: vec![
            gain("drv"),
            insert("insert:pedais", "pedais"),
            insert("insert:syn2", "syn2-main"),
            gain("amp"),
        ],
        di_output: None,
        loopers: vec![],
    }
}

/// Peak per output route index, measured on that route's own device channel
/// once the fade-in settled. Both loops are closed in software: what leaves on
/// a send this callback comes back on its return on the next one.
fn tail_peaks(chain: &Chain, tail_channels: [usize; 2]) -> Vec<f32> {
    let route_count = 4;
    let registry = registry();
    let runtimes = build_per_input_runtimes(
        chain,
        48_000.0,
        &HashMap::new(),
        &vec![DEFAULT_ELASTIC_TARGET; route_count],
        &registry,
    )
    .expect("the chain must build");
    assert_eq!(runtimes.len(), 1, "an insert chain is one runtime");
    let runtime = Arc::new(runtimes.into_iter().next().unwrap().1);

    let mut input = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
    let mut outputs: Vec<Vec<f32>> = (0..route_count)
        .map(|_| vec![0.0_f32; FRAMES * DEVICE_CHANNELS])
        .collect();
    let mut peaks = vec![0.0_f32; route_count];
    for i in 0..256 {
        for frame in input.chunks_exact_mut(DEVICE_CHANNELS) {
            frame[GUITAR_IN] = 0.5;
        }
        process_input_f32(&runtime, 0, &input, DEVICE_CHANNELS);
        for (route, out) in outputs.iter_mut().enumerate() {
            out.fill(0.0);
            process_output_f32(&runtime, route, out, DEVICE_CHANNELS);
        }
        // Close the loops for the next callback.
        for (f, frame) in input.chunks_exact_mut(DEVICE_CHANNELS).enumerate() {
            let at = f * DEVICE_CHANNELS;
            frame[PEDAIS_RETURN] = outputs[2][at + PEDAIS_SEND];
            frame[SYN2_RETURN[0]] = outputs[3][at + SYN2_SEND];
            frame[SYN2_RETURN[1]] = outputs[3][at + SYN2_SEND];
        }
        if i >= 128 {
            for (route, out) in outputs.iter().enumerate() {
                let ch = match route {
                    0 => tail_channels[0],
                    1 => tail_channels[1],
                    2 => PEDAIS_SEND,
                    _ => SYN2_SEND,
                };
                for frame in out.chunks_exact(DEVICE_CHANNELS) {
                    peaks[route] = peaks[route].max(frame[ch].abs());
                }
            }
        }
    }
    peaks
}

#[test]
fn with_inserts_enabled_every_tail_output_plays() {
    let peaks = tail_peaks(
        &owner_chain(&["guitarra-1", "guitarra-1-5050"]),
        [MAIN_OUT[0], ADAT_OUT[0]],
    );
    let (main, adat) = (peaks[0], peaks[1]);
    assert!(main > 0.01, "Main must play — peak was {main}");
    assert!(
        adat > 0.01,
        "the second binding's tail output (ADAT 3/4) must play too — peak was {adat}"
    );
}

#[test]
fn with_inserts_enabled_binding_order_does_not_matter() {
    let peaks = tail_peaks(
        &owner_chain(&["guitarra-1-5050", "guitarra-1"]),
        [ADAT_OUT[0], MAIN_OUT[0]],
    );
    let (adat, main) = (peaks[0], peaks[1]);
    assert!(
        adat > 0.01,
        "ADAT 3/4 (first now) must play — peak was {adat}"
    );
    assert!(main > 0.01, "Main (second now) must play — peak was {main}");
}
