//! #965 — an insert loop must not cost more latency than its own cushion.
//!
//! The owner's rig: ANAL+DIG on the Quantum HD 8 at 64 frames — guitar in,
//! insert to the SYN-2 (send ADAT out 9, return ADAT 2/3), then a preamp and
//! a cab IR, out to Main and the FRFR. Measured live: the send route AND both
//! tail routes sat at 1024 queued frames each (23 ms per hop, the ring's whole
//! capacity), against 512 on the acoustic chain next to it. Two defects add
//! up to it, each pinned below with the production build path.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use super::{process_input_f32, process_output_f32};
use crate::elastic_drift_guard::SLACK_FRAMES;
use crate::runtime_graph::{build_chain_runtime_state, build_per_input_runtimes};
use crate::runtime_state::ChainRuntimeState;

const QUANTUM: &str = "coreaudio:quantum";
const DEVICE_CHANNELS: usize = 26;
const FRAMES: usize = 64;
const GUITAR_IN: usize = 0;
const MAIN_OUT: [usize; 2] = [0, 1];
const SYN2_RETURN: [usize; 2] = [15, 16];
const SYN2_SEND: usize = 22;

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

fn cab(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_CAB.to_string(),
            model: "ir_test_fake".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn insert(id: &str, io: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".into(),
            io: io.into(),
        }),
    }
}

fn chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into()],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

/// Defect 1: the IR cold-start cushion (#592) is decided per CHAIN, so the
/// insert SEND — fed by the segment BEFORE the insert, which holds no IR at
/// all — got the same 512-frame cushion as the tail behind the cab. The send
/// was sized lean on purpose (`ELASTIC_MULTIPLIER_INSERT_SEND`); the chain-wide
/// floor silently undid that.
#[test]
fn the_insert_send_gets_no_ir_cushion_when_no_ir_feeds_it() {
    let owner = chain(vec![insert("insert:1", "syn2-main"), cab("cab")]);
    let tail_target = 128;
    let send_target = 64;
    let rt = build_chain_runtime_state(&owner, 44_100.0, &[tail_target, send_target], &registry())
        .expect("the chain must build");
    let routes = rt.output_routes.load();
    let send = routes[1].as_ref().expect("route 1 is the insert send");
    assert_eq!(
        send.buffer.target_level(),
        send_target,
        "the send route sits BEFORE the cab: nothing convolves into it, so it \
         must keep its own lean target, not the chain-wide IR cushion"
    );
    assert_eq!(
        send.buffer.len(),
        0,
        "and it must not be primed with silence either — every primed frame \
         is a frame of latency the loop pays for nothing"
    );
    let tail = routes[0].as_ref().expect("route 0 is the main tail");
    assert!(
        tail.buffer.target_level() >= 512,
        "the tail behind the cab keeps the #592 cushion"
    );
}

/// One route, no IR: the plain rig the second defect needs.
fn plain_runtime() -> Arc<ChainRuntimeState> {
    let runtimes = build_per_input_runtimes(
        &chain(vec![gain("amp")]),
        44_100.0,
        &HashMap::new(),
        &[TARGET],
        &registry(),
    )
    .expect("the chain must build");
    assert_eq!(runtimes.len(), 1, "one binding, one input = one runtime");
    Arc::new(runtimes.into_iter().next().unwrap().1)
}

const TARGET: usize = 128;

struct Rig {
    runtime: Arc<ChainRuntimeState>,
    input: Vec<f32>,
    out: Vec<f32>,
}

impl Rig {
    fn new() -> Self {
        Self {
            runtime: plain_runtime(),
            input: vec![0.0; FRAMES * DEVICE_CHANNELS],
            out: vec![0.0; FRAMES * DEVICE_CHANNELS],
        }
    }

    /// The input callback alone — the input stream running while the output
    /// stream has not started yet.
    fn input_period(&mut self, pulse: bool) {
        self.input.fill(0.0);
        if pulse {
            self.input[GUITAR_IN] = 0.5;
        }
        process_input_f32(&self.runtime, 0, &self.input, DEVICE_CHANNELS);
    }

    /// One device period with both streams running. Returns the first frame
    /// of this period carrying signal on the main route.
    fn period(&mut self, pulse: bool) -> Option<usize> {
        self.input_period(pulse);
        self.out.fill(0.0);
        process_output_f32(&self.runtime, 0, &mut self.out, DEVICE_CHANNELS);
        self.out
            .chunks_exact(DEVICE_CHANNELS)
            .position(|frame| frame[MAIN_OUT[0]].abs() > 1e-3)
    }

    /// Latency of the route, in frames, for a pulse sent now.
    fn pulse_latency(&mut self) -> usize {
        for p in 0..64 {
            if let Some(f) = self.period(p == 0) {
                return p * FRAMES + f;
            }
        }
        panic!("the pulse must come out of the route");
    }
}

/// Defect 2: the streams of a chain are started input first, output last, and
/// the output stream takes its time to deliver its first callback. Every
/// input period in between pushes a buffer nobody pops, so the ring is FULL by
/// the time the output runs — and the #953 guard, which learns the route's
/// resting level from the first clean window, ratified that full ring as
/// "normal". The route then carried the whole capacity as latency until the
/// chain was switched off and on.
#[test]
fn a_route_whose_input_ran_ahead_of_its_output_settles_back_to_its_cushion() {
    let mut rig = Rig::new();
    // The input stream delivers 16 periods before the output stream starts.
    for _ in 0..16 {
        rig.input_period(false);
    }
    // Then the rig plays for ~3 s.
    for _ in 0..2_000 {
        rig.period(false);
    }
    let latency = rig.pulse_latency();
    assert!(
        latency <= TARGET + SLACK_FRAMES,
        "the route kept {latency} frames of latency after its input stream ran \
         ahead of its output stream at start-up; it must settle to its own \
         cushion ({TARGET} frames) like a route whose streams started together"
    );
}
