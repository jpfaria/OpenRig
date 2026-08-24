//! #881 — an `Insert` block must never silence the chain.
//!
//! The reported bug is audible: adding an insert made the signal disappear. It
//! was not "no route" — the graph build indexed past the effective inputs and
//! panicked, so the chain had no runtime at all. These drive real buffers
//! through the built runtime and assert the guitar still comes out the tail.

use std::sync::Arc;

use super::tests::{insert_chain, insert_registry};
use super::{build_chain_runtime_state, process_input_f32, process_output_f32};
use crate::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use project::block::AudioBlockKind;
use project::chain::Chain;

const FRAMES: usize = 128;
const CHANNELS: usize = 2;

/// Push a steady tone through input 0 and read the tail output, letting the
/// fade-in settle first.
fn peak_through(chain: &Chain) -> f32 {
    let runtime = Arc::new(
        build_chain_runtime_state(
            chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &insert_registry(),
        )
        .expect("the chain must build a runtime"),
    );
    let input = vec![0.5_f32; FRAMES * CHANNELS];
    let mut output = vec![0.0_f32; FRAMES * CHANNELS];
    let mut peak = 0.0_f32;
    for i in 0..256 {
        process_input_f32(&runtime, 0, &input, CHANNELS);
        output.fill(0.0);
        process_output_f32(&runtime, 0, &mut output, CHANNELS);
        // Ignore the fade-in ramp; measure once the chain is running.
        if i >= 128 {
            peak = peak.max(output.iter().fold(0.0_f32, |m, s| m.max(s.abs())));
        }
    }
    peak
}

#[test]
fn an_unbound_insert_lets_the_chain_play() {
    let mut chain = insert_chain();
    let AudioBlockKind::Insert(ref mut ib) = chain.blocks[2].kind else {
        panic!("block 2 should be the insert");
    };
    ib.io = String::new();

    let peak = peak_through(&chain);

    assert!(
        peak > 0.01,
        "an insert with no E/S must be bypassed, not silence the chain — tail peak was {peak}"
    );
}

#[test]
fn an_insert_bound_to_a_missing_binding_lets_the_chain_play() {
    let mut chain = insert_chain();
    let AudioBlockKind::Insert(ref mut ib) = chain.blocks[2].kind else {
        panic!("block 2 should be the insert");
    };
    // A project moved from another machine: the id resolves to nothing here.
    ib.io = "loop_on_the_other_rig".into();

    let peak = peak_through(&chain);

    assert!(
        peak > 0.01,
        "a project whose insert binding is absent on this machine must still \
         play — tail peak was {peak}"
    );
}

#[test]
fn a_disabled_insert_lets_the_chain_play() {
    let mut chain = insert_chain();
    chain.blocks[2].enabled = false;

    let peak = peak_through(&chain);

    assert!(
        peak > 0.01,
        "a disabled insert is a bypass, not a cut — tail peak was {peak}"
    );
}
