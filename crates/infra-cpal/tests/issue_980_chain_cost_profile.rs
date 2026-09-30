//! Issue #980 — does the owner's ANAL+DIG chain itself ever cost more than a
//! buffer period, with the operating system taken out of the picture?
//!
//! On the Quantum the dsp-worker sometimes spends 2.3-6.5 ms of THREAD CPU on
//! a 64-frame buffer whose normal cost is 0.5-0.8 ms (see
//! `docs/audio-incidents/980-dsp-worker-late-underruns.md`). This test runs the
//! same chain on one thread, back to back, with no audio device, no sleeping
//! and no other runtime, and measures each call's thread CPU time — the whole
//! chain with one and two outputs, and each block alone — on a real guitar DI
//! and on silence, with each call's instructions, cycles and performance-core
//! share read from `proc_pid_rusage` (the chain runs on this process's only
//! busy thread).
//!
//! Measured 2026-09-28 (Apple Silicon, 5 P + 6 E cores, load 5-10): the median
//! buffer of the two-output chain is 7.05 M instructions in 1.14 M cycles —
//! 300 µs, 100% on a performance core, IPC 6.2 at 3.8 GHz. The slowest buffers
//! (0.6-2.6 ms, clustered, at different positions every run) execute the SAME
//! ~7.1 M instructions in 2-3x the cycles: either on an efficiency core
//! (0-46% of cycles on P, 2.6-2.8 GHz, IPC 2.1-2.7) or on a performance core
//! with IPC down to 2.5-4 (memory stalls). The chain never does bursty extra
//! work; the machine sometimes runs the same work 3-8x slower.
//!
//! Pinned here: the slowest buffers of the owner's chain execute at most twice
//! the median buffer's instructions — a block that starts doing periodic extra
//! work (an internal re-block, a denormal storm, a reallocation) fails it.
//!
//! macOS + release + `OPENRIG_OWNER_PLUGINS=<OpenRig-plugins/plugins/source>`;
//! no hardware needed. Otherwise it skips with a notice.
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use std::path::PathBuf;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::ChainRuntimeState;
use hw_harness::init_registry_with_root;
use project::block::{AudioBlock, AudioBlockKind};
use project::chain::Chain;

const RATE: f32 = 44_100.0;
const FRAMES: usize = 64;
const CHANNELS: usize = 12;
const PERIOD_US: u64 = 1_451;
const BUFFERS: usize = 20_000; // ~29 s of audio per case

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}
extern "C" {
    fn clock_gettime(clock: i32, ts: *mut Timespec) -> i32;
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut u64) -> i32;
    fn getpid() -> i32;
}
const CLOCK_THREAD_CPUTIME_ID: i32 = 16;
const RUSAGE_INFO_V6: i32 = 6;

/// Process counters around one call: this test runs the chain on its only
/// busy thread, so the process deltas are that call's — instructions, cycles
/// and how much of it ran on a performance core.
#[derive(Clone, Copy, Default)]
struct Counters {
    instructions: u64,
    cycles: u64,
    p_cycles: u64,
}

fn counters() -> Counters {
    let mut buf = [0u64; 96];
    unsafe { proc_pid_rusage(getpid(), RUSAGE_INFO_V6, buf.as_mut_ptr()) };
    Counters {
        instructions: buf[31],
        cycles: buf[32],
        p_cycles: buf[41],
    }
}

fn thread_cpu_ns() -> u64 {
    let mut ts = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { clock_gettime(CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

fn endpoint(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn registry(outputs: usize) -> Vec<IoBinding> {
    let mut outs = vec![endpoint("main", ChannelMode::Stereo, &[0, 1])];
    if outputs == 2 {
        outs.push(endpoint("out 2", ChannelMode::Stereo, &[10, 11]));
    }
    vec![IoBinding {
        id: "guitarra-1".into(),
        name: "guitarra-1".into(),
        inputs: vec![endpoint("in", ChannelMode::Mono, &[0])],
        outputs: outs,
    }]
}

fn owner_blocks() -> Vec<AudioBlock> {
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets/issue_980_owner_anal_dig.yaml");
    infra_yaml::load_chain_preset_file(&preset)
        .expect("preset")
        .blocks
}

fn model_of(block: &AudioBlock) -> String {
    match &block.kind {
        AudioBlockKind::Core(c) => c.model.clone(),
        _ => "?".into(),
    }
}

fn runtime(blocks: Vec<AudioBlock>, outputs: usize) -> Arc<ChainRuntimeState> {
    let chain = Chain {
        id: ChainId("issue-980-cost".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into()],
        blocks,
        di_output: None,
        loopers: vec![],
        mix: Default::default(),
    };
    Arc::new(
        engine::runtime::build_chain_runtime_state(&chain, RATE, &[64, 64], &registry(outputs))
            .expect("the chain must build"),
    )
}

fn guitar_di() -> Vec<f32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/di-loops/phil-STRATO-green_day.wav");
    let mut reader = hound::WavReader::open(&path).expect("DI loop");
    let spec = reader.spec();
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.unwrap() as f32 / max)
                .collect()
        }
    };
    raw.chunks(spec.channels as usize).map(|f| f[0]).collect()
}

struct Profile {
    p50: u64,
    p99: u64,
    p999: u64,
    max: u64,
    over_period: usize,
    over_half: usize,
    worst_at: Vec<usize>,
    /// (cpu µs, instructions, cycles, P-core cycles) of the median buffer
    /// and of the eight slowest.
    median_counts: (u64, u64, u64, u64),
    worst_counts: Vec<(u64, u64, u64, u64)>,
}

/// Runs `BUFFERS` buffers of `source` (looped) through `runtime` on this
/// thread and profiles each input call's thread CPU time (µs).
fn profile(runtime: &Arc<ChainRuntimeState>, source: &[f32], outputs: usize) -> Profile {
    let mut input = vec![0.0f32; FRAMES * CHANNELS];
    let mut out = vec![0.0f32; FRAMES * CHANNELS];
    let mut costs = Vec::with_capacity(BUFFERS);
    let mut counts: Vec<(u64, u64, u64, u64)> = Vec::with_capacity(BUFFERS);
    let mut pos = 0usize;
    // Warm up so first-call initialisation is not counted.
    for i in 0..(BUFFERS + 500) {
        for f in 0..FRAMES {
            input[f * CHANNELS] = if source.is_empty() {
                0.0
            } else {
                source[pos % source.len()]
            };
            pos += 1;
        }
        let c0 = counters();
        let t0 = thread_cpu_ns();
        engine::runtime::process_input_f32(runtime, 0, &input, CHANNELS);
        let cost_us = (thread_cpu_ns() - t0) / 1_000;
        let c1 = counters();
        for route in 0..outputs {
            engine::runtime::process_output_f32(runtime, route, &mut out, CHANNELS);
        }
        if i >= 500 {
            costs.push(cost_us);
            counts.push((
                cost_us,
                c1.instructions - c0.instructions,
                c1.cycles - c0.cycles,
                c1.p_cycles - c0.p_cycles,
            ));
        }
    }
    let mut by_cost = counts.clone();
    by_cost.sort_unstable_by_key(|c| c.0);
    let median_counts = by_cost[by_cost.len() / 2];
    let worst_counts: Vec<_> = by_cost.iter().rev().take(8).copied().collect();
    let mut sorted = costs.clone();
    sorted.sort_unstable();
    let pct = |q: f64| sorted[((sorted.len() - 1) as f64 * q) as usize];
    let mut worst: Vec<(u64, usize)> = costs.iter().copied().zip(0..).collect();
    worst.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    Profile {
        p50: pct(0.5),
        p99: pct(0.99),
        p999: pct(0.999),
        max: *sorted.last().unwrap_or(&0),
        over_period: costs.iter().filter(|&&c| c > PERIOD_US).count(),
        over_half: costs.iter().filter(|&&c| c > PERIOD_US / 2).count(),
        worst_at: worst.iter().take(8).map(|&(_, i)| i).collect(),
        median_counts,
        worst_counts,
    }
}

#[test]
fn the_owners_chain_never_costs_more_than_a_period_per_buffer() {
    let Some(plugins_root) = std::env::var_os("OPENRIG_OWNER_PLUGINS") else {
        eprintln!(
            "[#980 cost] SKIPPED — needs OPENRIG_OWNER_PLUGINS=<OpenRig-plugins/plugins/source>."
        );
        return;
    };
    let plugins_root = PathBuf::from(plugins_root);
    init_registry_with_root(&plugins_root);
    project::vst3_editor::init_vst3_catalog(RATE as f64, &[plugins_root]);

    let blocks = owner_blocks();
    let di = guitar_di();
    let silence: Vec<f32> = Vec::new();

    let mut cases: Vec<(String, Vec<AudioBlock>, usize)> = vec![
        ("full chain, 2 outputs".into(), blocks.clone(), 2),
        ("full chain, 1 output".into(), blocks.clone(), 1),
    ];
    for block in &blocks {
        cases.push((format!("{} alone", model_of(block)), vec![block.clone()], 1));
    }

    let mut worst_instruction_ratio = 0.0f64;
    for (name, case_blocks, outputs) in cases {
        for (input_name, source) in [("guitar", &di), ("silence", &silence)] {
            let runtime = runtime(case_blocks.clone(), outputs);
            let p = profile(&runtime, source, outputs);
            eprintln!(
                "[#980 cost] {name:<55} {input_name:<8} cpu us p50 {:>5} p99 {:>5} p99.9 {:>5} max {:>6} | >period {:>4} >half {:>5} | worst buffers at {:?}",
                p.p50, p.p99, p.p999, p.max, p.over_period, p.over_half, p.worst_at
            );
            if name.starts_with("full chain, 2") {
                let median_instructions = p.median_counts.1.max(1) as f64;
                for w in &p.worst_counts {
                    worst_instruction_ratio =
                        worst_instruction_ratio.max(w.1 as f64 / median_instructions);
                }
                let show = |(us, ins, cyc, pcyc): (u64, u64, u64, u64)| {
                    format!(
                        "{us}us {:.2}M instr {:.2}M cycles ({}% on P) IPC {:.2} {:.2} GHz",
                        ins as f64 / 1e6,
                        cyc as f64 / 1e6,
                        if cyc == 0 { 0 } else { pcyc * 100 / cyc },
                        if cyc == 0 {
                            0.0
                        } else {
                            ins as f64 / cyc as f64
                        },
                        if us == 0 {
                            0.0
                        } else {
                            cyc as f64 / (us as f64 * 1e3)
                        }
                    )
                };
                eprintln!("[#980 cost]   median buffer: {}", show(p.median_counts));
                for w in &p.worst_counts {
                    eprintln!("[#980 cost]   slow buffer:   {}", show(*w));
                }
            }
        }
    }

    assert!(
        worst_instruction_ratio <= 2.0,
        "a slow buffer of the owner's chain executed {worst_instruction_ratio:.2}x the median \
         buffer's instructions: a block is doing bursty extra work"
    );
}
