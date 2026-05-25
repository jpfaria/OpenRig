//! Issue #516: selecting an `OutputBlock` mode of `Mono` must not silence
//! the device. The user reports zero audible output the moment the chain's
//! output is switched from stereo to a single mono channel.
//!
//! Existing coverage in `audio_signal_integrity_tests.rs` only exercises a
//! 1-channel device buffer (`output_total_channels = 1`), which masks the
//! real-world case: a stereo audio interface (`output_total_channels = 2`)
//! routing a `ChainOutputMode::Mono` chain to a single channel. The frame
//! buffer is interleaved L/R for the device; the engine must still write
//! audible samples into the channels addressed by `OutputEntry.channels`.
//!
//! These tests pin the user-facing invariant: with non-silent input, the
//! captured device buffer MUST contain non-zero samples on the routed
//! channels. Silence is a regression.

use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use engine::runtime::{process_input_f32, process_output_f32, process_output_f32_mixed};
use engine::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use engine::runtime_graph::{build_chain_runtime_state, update_chain_runtime_state};
use engine::runtime_state::ChainRuntimeState;
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};

const SR: f32 = 48_000.0;
const BUFFER_FRAMES: usize = 64;

fn input_block(mode: ChainInputMode, channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("input:0".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode,
                channels,
            }],
        }),
    }
}

/// Mirrors the user's real chain: a single `InputBlock` containing
/// multiple `InputEntry` items, each one a mono entry on its own device
/// (Scarlett ch 0, Scarlett ch 1, TEYUN ch 0 in the live project).
/// Each entry becomes its own isolated `ChainRuntimeState` (per CLAUDE.md
/// invariant #4 + issue #350), and the output is summed by
/// `process_output_f32_mixed`.
fn multi_input_block(entries: Vec<InputEntry>) -> AudioBlock {
    AudioBlock {
        id: BlockId("input:0".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries,
        }),
    }
}

fn mono_entry(device: &str, ch: usize) -> InputEntry {
    InputEntry {
        device_id: DeviceId(device.into()),
        mode: ChainInputMode::Mono,
        channels: vec![ch],
    }
}

fn output_block(mode: ChainOutputMode, channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("output:0".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId("dev".into()),
                mode,
                channels,
            }],
        }),
    }
}

fn chain_named(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("issue #516 mono-output audibility".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks,
    }
}

fn build_runtime(chain: &Chain) -> Arc<ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(chain, SR, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime state must build for test chain"),
    )
}

fn fill_sine(buf: &mut [f32], channels: usize, phase: &mut f32) {
    let incr = 2.0 * std::f32::consts::PI * 220.0 / SR;
    let frames = buf.len() / channels;
    for f in 0..frames {
        let s = 0.5 * phase.sin();
        *phase += incr;
        if *phase > std::f32::consts::TAU {
            *phase -= std::f32::consts::TAU;
        }
        for c in 0..channels {
            buf[f * channels + c] = s;
        }
    }
}

/// Drives the chain through a sine input long enough to clear the FADE_IN
/// warmup and returns the steady-state output buffer (interleaved across
/// `device_channels`). The capture skips the first `warmup` callbacks so
/// the assertion sees only the steady-state signal.
fn drive_capture(
    runtime: &Arc<ChainRuntimeState>,
    input_channels: usize,
    device_channels: usize,
) -> Vec<f32> {
    const CALLBACKS: usize = 32;
    const WARMUP: usize = 8;

    let mut input_buf = vec![0.0_f32; BUFFER_FRAMES * input_channels];
    let mut output_buf = vec![0.0_f32; BUFFER_FRAMES * device_channels];
    let mut captured: Vec<f32> =
        Vec::with_capacity((CALLBACKS - WARMUP) * BUFFER_FRAMES * device_channels);
    let mut phase = 0.0_f32;

    for cb in 0..CALLBACKS {
        fill_sine(&mut input_buf, input_channels, &mut phase);
        process_input_f32(runtime, 0, &input_buf, input_channels);
        process_output_f32(runtime, 0, &mut output_buf, device_channels);
        if cb >= WARMUP {
            captured.extend_from_slice(&output_buf);
        }
    }
    captured
}

fn channel_peak(captured: &[f32], device_channels: usize, channel: usize) -> f32 {
    let frames = captured.len() / device_channels;
    let mut peak = 0.0_f32;
    for f in 0..frames {
        let s = captured[f * device_channels + channel].abs();
        if s > peak {
            peak = s;
        }
    }
    peak
}

/// Repro for #516: mono input → `ChainOutputMode::Mono` routed to channel 0
/// of a stereo audio device must produce audible samples on channel 0.
#[test]
fn mono_output_on_stereo_device_writes_audio() {
    let chain = chain_named(
        "issue-516-mono-on-stereo-device",
        vec![
            input_block(ChainInputMode::Mono, vec![0]),
            output_block(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtime = build_runtime(&chain);
    let captured = drive_capture(&runtime, 1, 2);

    let peak_ch0 = channel_peak(&captured, 2, 0);
    assert!(
        peak_ch0 > 0.05,
        "mono output on stereo device produced silence on channel 0 (peak |ch0| = {peak_ch0})"
    );
}

/// Stereo input mixed down through `ChainOutputMode::Mono` routed to
/// channel 0 of a stereo device must still produce audible samples there.
#[test]
fn stereo_input_mono_output_on_stereo_device_writes_audio() {
    let chain = chain_named(
        "issue-516-stereo-in-mono-out",
        vec![
            input_block(ChainInputMode::Stereo, vec![0, 1]),
            output_block(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtime = build_runtime(&chain);
    let captured = drive_capture(&runtime, 2, 2);

    let peak_ch0 = channel_peak(&captured, 2, 0);
    assert!(
        peak_ch0 > 0.05,
        "stereo→mono output on stereo device produced silence on channel 0 (peak |ch0| = {peak_ch0})"
    );
}

/// User has a multi-output audio interface (8 channels). They selected
/// `ChainOutputMode::Mono` routed to a single physical output. The buffer
/// the device hands us is interleaved across ALL 8 channels. The picked
/// channel must receive audio.
#[test]
fn mono_output_on_multichannel_interface_writes_audio() {
    let chain = chain_named(
        "issue-516-mono-on-multichannel",
        vec![
            input_block(ChainInputMode::Mono, vec![0]),
            output_block(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtime = build_runtime(&chain);
    let captured = drive_capture(&runtime, 1, 8);

    let peak_ch0 = channel_peak(&captured, 8, 0);
    assert!(
        peak_ch0 > 0.05,
        "mono output on 8-ch interface produced silence on ch 0 (peak |ch0| = {peak_ch0})"
    );
}

/// Right-channel mono output: user routes the mono signal to channel 1
/// (right monitor only). Must produce audible samples on channel 1.
#[test]
fn mono_output_routed_to_right_channel_writes_audio() {
    let chain = chain_named(
        "issue-516-mono-right-only",
        vec![
            input_block(ChainInputMode::Mono, vec![0]),
            output_block(ChainOutputMode::Mono, vec![1]),
        ],
    );
    let runtime = build_runtime(&chain);
    let captured = drive_capture(&runtime, 1, 2);

    let peak_ch1 = channel_peak(&captured, 2, 1);
    assert!(
        peak_ch1 > 0.05,
        "mono output routed to channel 1 produced silence on ch 1 (peak |ch1| = {peak_ch1})"
    );
}

/// UI hazard: the `on_select_output_mode` callback updates `output.mode`
/// but does NOT touch `output.channels`. A user who was in Stereo mode
/// with channels=[0,1] and toggled the mode to Mono ends up with mode=Mono
/// AND channels=[0,1]. The engine must still emit audio.
#[test]
fn mono_output_with_two_channels_still_selected_writes_audio() {
    let chain = chain_named(
        "issue-516-mono-with-stereo-channels-selected",
        vec![
            input_block(ChainInputMode::Mono, vec![0]),
            output_block(ChainOutputMode::Mono, vec![0, 1]),
        ],
    );
    let runtime = build_runtime(&chain);
    let captured = drive_capture(&runtime, 1, 2);

    let peak_ch0 = channel_peak(&captured, 2, 0);
    assert!(
        peak_ch0 > 0.05,
        "mono mode with channels=[0,1] selected produced silence on ch 0 (peak |ch0| = {peak_ch0})"
    );
}

/// Pure mono pipeline on a 1-channel device — historically covered by
/// `audio_signal_integrity_tests`, repeated here as a baseline so any
/// regression in the broader test file is immediately localized.
#[test]
fn mono_input_mono_output_on_mono_device_writes_audio() {
    let chain = chain_named(
        "issue-516-pure-mono",
        vec![
            input_block(ChainInputMode::Mono, vec![0]),
            output_block(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtime = build_runtime(&chain);
    let captured = drive_capture(&runtime, 1, 1);

    let peak_ch0 = channel_peak(&captured, 1, 0);
    assert!(
        peak_ch0 > 0.05,
        "pure mono pipeline produced silence (peak |ch0| = {peak_ch0})"
    );
}

/// Stereo input → mono output → mono device. The internal stereo bus must
/// be mixed down (per CLAUDE.md invariant #5) and written to the single
/// device channel.
#[test]
fn stereo_input_mono_output_on_mono_device_writes_audio() {
    let chain = chain_named(
        "issue-516-stereo-in-mono-device",
        vec![
            input_block(ChainInputMode::Stereo, vec![0, 1]),
            output_block(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtime = build_runtime(&chain);
    let captured = drive_capture(&runtime, 2, 1);

    let peak_ch0 = channel_peak(&captured, 1, 0);
    assert!(
        peak_ch0 > 0.05,
        "stereo→mono on mono device produced silence (peak |ch0| = {peak_ch0})"
    );
}

/// User flow: open chain in Stereo mode → audio plays → toggle output to
/// Mono with a single channel selected → audio MUST still play. The engine
/// path goes through `update_chain_runtime_state` (param/preset/io edit),
/// not a fresh `build_chain_runtime_state`. This is the closest test to
/// the real UI flow that triggered issue #516.
#[test]
fn toggle_stereo_to_mono_via_update_keeps_audio_audible() {
    let stereo_chain = chain_named(
        "issue-516-toggle-stereo-mono",
        vec![
            input_block(ChainInputMode::Mono, vec![0]),
            output_block(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let runtime = build_runtime(&stereo_chain);

    // Sanity: stereo baseline emits audio on the chosen channel.
    let baseline = drive_capture(&runtime, 1, 2);
    let baseline_peak = channel_peak(&baseline, 2, 0);
    assert!(
        baseline_peak > 0.05,
        "baseline stereo chain produced silence (peak |ch0| = {baseline_peak})"
    );

    // User toggles output to Mono with a single channel still selected
    // (matches `on_select_output_mode` callback: only `mode` is changed,
    // channels are left untouched — here we explicitly drop down to a
    // single channel as the user did).
    let mono_chain = chain_named(
        "issue-516-toggle-stereo-mono",
        vec![
            input_block(ChainInputMode::Mono, vec![0]),
            output_block(ChainOutputMode::Mono, vec![0]),
        ],
    );
    update_chain_runtime_state(&runtime, &mono_chain, SR, false, &[DEFAULT_ELASTIC_TARGET])
        .expect("runtime must accept Stereo→Mono toggle");

    let captured = drive_capture(&runtime, 1, 2);
    let peak_ch0 = channel_peak(&captured, 2, 0);
    assert!(
        peak_ch0 > 0.05,
        "Stereo→Mono toggle silenced the chain (peak |ch0| = {peak_ch0})"
    );
}

/// Issue #516 repro from the live project YAML:
///
/// A single `InputBlock` with **three mono entries** (Scarlett ch 0,
/// Scarlett ch 1, TEYUN ch 0) feeding a chain whose `OutputBlock` is
/// `ChainOutputMode::Mono` with `channels: [0]` on a stereo device. Each
/// input entry becomes its own isolated `ChainRuntimeState`; the device
/// runs `process_output_f32_mixed` which sums them. The summed buffer
/// must contain audible samples on channel 0.
#[test]
fn multi_input_mono_output_via_mixed_path_writes_audio() {
    let chain = chain_named(
        "issue-516-multi-input-mono-out",
        vec![
            multi_input_block(vec![
                mono_entry("scarlett", 0),
                mono_entry("scarlett", 1),
                mono_entry("teyun", 0),
            ]),
            output_block(ChainOutputMode::Mono, vec![0]),
        ],
    );

    // Build one runtime per input entry — that's exactly what infra-cpal
    // does in `chain_resolve` for multi-input chains (#350 phase 3).
    let runtimes: Vec<Arc<ChainRuntimeState>> = (0..3).map(|_| build_runtime(&chain)).collect();

    const DEVICE_CHANNELS: usize = 2;
    const CALLBACKS: usize = 32;
    const WARMUP: usize = 8;
    let mut phase = 0.0_f32;
    let mut input_buf = vec![0.0_f32; BUFFER_FRAMES];
    let mut output_buf = vec![0.0_f32; BUFFER_FRAMES * DEVICE_CHANNELS];
    let mut scratch = vec![0.0_f32; BUFFER_FRAMES * DEVICE_CHANNELS];
    let mut captured: Vec<f32> =
        Vec::with_capacity((CALLBACKS - WARMUP) * BUFFER_FRAMES * DEVICE_CHANNELS);

    for cb in 0..CALLBACKS {
        fill_sine(&mut input_buf, 1, &mut phase);
        for runtime in &runtimes {
            process_input_f32(runtime, 0, &input_buf, 1);
        }
        process_output_f32_mixed(&runtimes, 0, &mut output_buf, DEVICE_CHANNELS, &mut scratch);
        if cb >= WARMUP {
            captured.extend_from_slice(&output_buf);
        }
    }

    let peak_ch0 = channel_peak(&captured, DEVICE_CHANNELS, 0);
    assert!(
        peak_ch0 > 0.05,
        "multi-input mono output produced silence on ch 0 via process_output_f32_mixed \
         (peak |ch0| = {peak_ch0})"
    );
}

/// Same scenario but with the SETLIST chain shape (single input entry, mono
/// in, mono out). This goes through the `[runtime]` fast path of
/// `process_output_f32_mixed` and must still produce audio.
#[test]
fn single_input_mono_output_via_mixed_path_writes_audio() {
    let chain = chain_named(
        "issue-516-single-input-mono-out-mixed",
        vec![
            input_block(ChainInputMode::Mono, vec![0]),
            output_block(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtimes = vec![build_runtime(&chain)];

    const DEVICE_CHANNELS: usize = 2;
    const CALLBACKS: usize = 32;
    const WARMUP: usize = 8;
    let mut phase = 0.0_f32;
    let mut input_buf = vec![0.0_f32; BUFFER_FRAMES];
    let mut output_buf = vec![0.0_f32; BUFFER_FRAMES * DEVICE_CHANNELS];
    let mut scratch = vec![0.0_f32; BUFFER_FRAMES * DEVICE_CHANNELS];
    let mut captured: Vec<f32> =
        Vec::with_capacity((CALLBACKS - WARMUP) * BUFFER_FRAMES * DEVICE_CHANNELS);

    for cb in 0..CALLBACKS {
        fill_sine(&mut input_buf, 1, &mut phase);
        process_input_f32(&runtimes[0], 0, &input_buf, 1);
        process_output_f32_mixed(&runtimes, 0, &mut output_buf, DEVICE_CHANNELS, &mut scratch);
        if cb >= WARMUP {
            captured.extend_from_slice(&output_buf);
        }
    }

    let peak_ch0 = channel_peak(&captured, DEVICE_CHANNELS, 0);
    assert!(
        peak_ch0 > 0.05,
        "single-input mono output via mixed path produced silence (peak |ch0| = {peak_ch0})"
    );
}

/// DualMono input → mono output: two independent guitars summed into one
/// channel. The mixdown should still produce audio.
#[test]
fn dual_mono_input_mono_output_writes_audio() {
    let chain = chain_named(
        "issue-516-dual-mono-in-mono-out",
        vec![
            input_block(ChainInputMode::DualMono, vec![0, 1]),
            output_block(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtime = build_runtime(&chain);
    let captured = drive_capture(&runtime, 2, 2);

    let peak_ch0 = channel_peak(&captured, 2, 0);
    assert!(
        peak_ch0 > 0.05,
        "dual-mono → mono output produced silence on ch 0 (peak |ch0| = {peak_ch0})"
    );
}
