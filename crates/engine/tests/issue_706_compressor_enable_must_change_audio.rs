//! Issue #706 — "enabling the native compressor changes nothing in the
//! sound". The DSP itself compresses (see
//! `block-dyn::issue_706_user_params_must_compress_dynamics`), so this
//! test reproduces one level up: the user's live flow. A chain runtime
//! is built with the compressor present but DISABLED (bypass), then the
//! block is enabled in place via the same fast path the GUI toggle uses
//! (`set_block_enabled`, issue #522). After the click-safe fade settles,
//! the processed audio MUST show the compression: the loud/quiet RMS
//! ratio of a two-level test signal has to shrink versus bypass.
//!
//! Params are the user's exact rig values (attack 10 ms, release 80 ms,
//! ratio 4:1, threshold 70, mix 100, makeup 50). The two-level signal
//! makes the assertion makeup-gain-proof: makeup shifts both passages
//! equally, while real compression shrinks their ratio.

use std::sync::{Arc, Once};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::value_objects::ParameterValue;
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use engine::runtime_block_toggle::set_block_enabled;
use engine::runtime_graph::{build_chain_runtime_state, update_chain_runtime_state};
use engine::runtime_state::ChainRuntimeState;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

const SR: f32 = 48_000.0;
const BUF: usize = 64;
const BLOCK_ID: &str = "issue706:compressor";

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        block_dyn::register_natives();
    });
}

fn user_compressor_params() -> ParameterSet {
    let schema = schema_for_block_model("dynamics", "compressor_studio_clean")
        .expect("compressor schema must exist");
    let mut ps = ParameterSet::default();
    ps.insert("attack_ms", ParameterValue::Float(10.0));
    ps.insert("release_ms", ParameterValue::Float(80.0));
    ps.insert("ratio", ParameterValue::Float(4.0));
    ps.insert("threshold", ParameterValue::Float(70.0));
    ps.insert("mix", ParameterValue::Float(100.0));
    ps.insert("makeup_gain", ParameterValue::Float(50.0));
    ps.normalized_against(&schema)
        .expect("user's rig params must normalize")
}

fn registry() -> Vec<domain::io_binding::IoBinding> {
    vec![domain::io_binding::IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![domain::io_binding::IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn chain_with_disabled_compressor() -> Chain {
    Chain {
        id: ChainId("issue706-chain".into()),
        description: Some("issue #706 compressor enable".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![AudioBlock {
            id: BlockId(BLOCK_ID.into()),
            enabled: false, // user starts with the block off
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "dynamics".into(),
                model: "compressor_studio_clean".into(),
                params: user_compressor_params(),
            }),
        }],
    }
}

/// Drive `seconds` of a 440 Hz tone at `amp` through the runtime and
/// return the left-channel RMS of the LAST half of the driven span (the
/// first half absorbs fades, attack and the elastic-buffer transient).
fn drive_and_measure(runtime: &Arc<ChainRuntimeState>, amp: f32, seconds: f32) -> f32 {
    let callbacks = ((SR * seconds) as usize) / BUF;
    let mut phase = 0.0_f32;
    let step = 2.0 * std::f32::consts::PI * 440.0 / SR;
    let mut input = vec![0.0_f32; BUF];
    let mut output = vec![0.0_f32; BUF * 2];
    let mut sum_sq = 0.0_f64;
    let mut count = 0_usize;
    for cb in 0..callbacks {
        for s in input.iter_mut() {
            *s = amp * phase.sin();
            phase += step;
        }
        process_input_f32(runtime, 0, &input, 1);
        process_output_f32(runtime, 0, &mut output, 2);
        if cb >= callbacks / 2 {
            for frame in output.chunks_exact(2) {
                sum_sq += (frame[0] as f64) * (frame[0] as f64);
                count += 1;
            }
        }
    }
    ((sum_sq / count.max(1) as f64) as f32).sqrt()
}

#[test]
fn issue_706_enabling_compressor_live_must_compress_the_audio() {
    init_registry();
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain_with_disabled_compressor(),
            SR,
            &[DEFAULT_ELASTIC_TARGET],
            &registry(),
        )
        .expect("runtime should build"),
    );

    // Bypass profile: block disabled.
    let quiet_bypass = drive_and_measure(&runtime, 0.05, 0.5);
    let loud_bypass = drive_and_measure(&runtime, 0.9, 0.5);
    let ratio_bypass = loud_bypass / quiet_bypass;

    // The user's action: enable the block on the LIVE runtime. This
    // mirrors `adapter-gui::sync_block_toggle` exactly: try the #522
    // fast path first; when it declines (block had no live processor),
    // fall back to the full runtime update with the new chain state —
    // the same path `sync_live_chain_runtime` takes.
    if set_block_enabled(&runtime, &BlockId(BLOCK_ID.into()), true).is_err() {
        let mut chain = chain_with_disabled_compressor();
        for block in &mut chain.blocks {
            if block.id.0 == BLOCK_ID {
                block.enabled = true;
            }
        }
        update_chain_runtime_state(
            &runtime,
            &chain,
            SR,
            false,
            &[DEFAULT_ELASTIC_TARGET],
            &registry(),
        )
        .expect("fallback runtime update must succeed");
    }

    // Let the click-safe fade fully settle before measuring.
    let _ = drive_and_measure(&runtime, 0.9, 0.3);

    let quiet_on = drive_and_measure(&runtime, 0.05, 0.5);
    let loud_on = drive_and_measure(&runtime, 0.9, 0.5);
    let ratio_on = loud_on / quiet_on;

    assert!(
        ratio_on < ratio_bypass * 0.9,
        "enabling compressor_studio_clean on the live runtime changed nothing: \
         loud/quiet RMS ratio bypass={ratio_bypass:.2} enabled={ratio_on:.2} \
         (quiet {quiet_bypass:.4}->{quiet_on:.4}, loud {loud_bypass:.4}->{loud_on:.4}) \
         — the toggle is audibly a no-op (issue #706)"
    );
}
