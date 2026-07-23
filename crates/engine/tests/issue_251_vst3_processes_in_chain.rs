//! #251 — the reported symptom: "o VST3 não está entrando na chain, não
//! funciona". A VST3 effect block must actually build and process audio when
//! its chain is rendered, not fault into a pass-through bypass.
//!
//! Requires ValhallaSupermassive installed. Skips (passes) when absent so CI
//! without the plugin stays green; run locally where the bundle exists.

use block_core::param::ParameterSet;
use domain::ids::{BlockId, ChainId};
use engine::offline::render_chain;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;

const SR: f32 = 48_000.0;
const MODEL_ID: &str = "vst3:ValhallaSupermassive:ValhallaSupermassive";

fn chain_with_blocks(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("issue-251 vst3 in chain".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

fn sine_input(frames: usize) -> Vec<[f32; 2]> {
    (0..frames)
        .map(|n| {
            let t = n as f32 / SR;
            let s = 0.2 * (2.0 * std::f32::consts::PI * 220.0 * t).sin();
            [s, s]
        })
        .collect()
}

#[test]
fn vst3_reverb_processes_audio_in_render_chain() {
    vst3_host::init_vst3_catalog(SR as f64, &[]);
    if vst3_host::find_vst3_plugin(MODEL_ID).is_none() {
        eprintln!("ValhallaSupermassive not installed — skipping VST3-in-chain repro");
        return;
    }

    let input = sine_input(8192);

    // Passthrough chain: output equals input.
    let chain_a = chain_with_blocks("pass", vec![]);
    let a = render_chain(&chain_a, SR, &input, 256, 0).expect("passthrough renders");

    // Same input, now with the Valhalla reverb block.
    let vst3_block = AudioBlock {
        id: BlockId("verb".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_VST3.into(),
            model: MODEL_ID.into(),
            params: ParameterSet::default(),
        }),
    };
    let chain_b = chain_with_blocks("verb", vec![vst3_block]);
    let b = render_chain(&chain_b, SR, &input, 256, 0).expect("vst3 chain renders");

    assert!(
        b.faulted_blocks.is_empty(),
        "BUG #251: the VST3 block faulted instead of processing — it did not \
         enter the chain. Faulted: {:?}",
        b.faulted_blocks
    );
    assert_ne!(
        a.samples, b.samples,
        "BUG #251: with-VST3 output is byte-identical to passthrough — the \
         plugin is a no-op bypass, i.e. it never processed."
    );
}
