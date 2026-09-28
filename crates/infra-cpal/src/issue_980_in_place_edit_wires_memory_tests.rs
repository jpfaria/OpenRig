//! Issue #980 (review of #981) — a live edit that updates a running chain IN
//! PLACE must get its memory wired at once, like a runtime that goes live
//! through a new slot or a publish.
//!
//! The owner's ANAL+DIG holds two VST3 reverbs, so every edit on it — a
//! preset, a block switched on, a NAM or IR swapped — takes the #779 in-place
//! path (`request_offthread_rebuild_if_live`, VST3 branch): fresh nodes, no
//! new slot, no publish, so the memory keeper was never woken and the new
//! processors' memory waited for the next periodic pass. A resync that keeps
//! the streams (`upsert_chain` with an unchanged signature) did the same. The
//! hardware trace showed the kernel compressing a new chain's pages inside
//! exactly that window.
//!
//! The probe is a buffer allocated just before the edit: once the edit has
//! woken the keeper it must be wired within a second — the periodic pass
//! comes only every 5 s. Run alone to see the red: other tests in this
//! binary wake the keeper too.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use domain::ids::{BlockId, DeviceId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use super::controller_live_edit_replicates_user_report_tests::{
    controller_with_active_chain, gain_chain, init_registry,
};
use crate::memory_residency_keeper::keep_resident;
use crate::memory_wiring::memory_wiring_tests::{touched_buffer, user_wired_count};
use crate::resolved::ResolvedChainAudioConfig;

/// Longer than a keeper wake-up pass plus any follow-up it runs.
const KEEPER_SETTLES: Duration = Duration::from_millis(2_500);
/// Well under the keeper's 5 s periodic pass.
const AT_ONCE: Duration = Duration::from_secs(1);

/// The gain chain plus a VST3 block switched off: `chain_contains_vst3` sends
/// every live edit down the in-place path, and a disabled block builds as a
/// bypass node, so no plugin binary is needed.
fn chain_with_a_vst3(volume_pct: f32) -> Chain {
    let mut chain = gain_chain(volume_pct);
    chain.blocks.push(AudioBlock {
        id: BlockId("issue980:vst3-off".into()),
        enabled: false,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_VST3.into(),
            model: "vst3:Missing:Missing".into(),
            params: ParameterSet::default(),
        }),
    });
    chain
}

fn project_with(chain: &Chain) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain.clone()],
        midi: None,
    }
}

fn wired_within(buffer: &[f32], within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if user_wired_count(buffer.as_ptr() as *const u8) > 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn a_live_edit_on_a_chain_with_a_vst3_wires_its_memory_at_once() {
    init_registry();
    keep_resident();
    let chain = chain_with_a_vst3(100.0);
    let controller_chain = chain.clone();
    let mut controller = controller_with_active_chain(&controller_chain);
    std::thread::sleep(KEEPER_SETTLES);

    let new_processor_memory = touched_buffer();
    let in_place = controller
        .request_offthread_rebuild_if_live(&project_with(&chain), &chain_with_a_vst3(50.0))
        .expect("live edit");
    assert!(in_place, "a VST3 chain's live edit takes the in-place path");
    assert!(
        wired_within(&new_processor_memory, AT_ONCE),
        "memory a live edit on a VST3 chain brings in must be wired within \
         1 s, not at the keeper's next 5 s pass"
    );
}

#[test]
fn a_resync_that_keeps_the_streams_wires_the_edited_chain_at_once() {
    init_registry();
    keep_resident();
    let chain = gain_chain(100.0);
    let mut controller = controller_with_active_chain(&chain);
    let signature = controller
        .active_chains
        .get(&chain.id)
        .expect("active chain")
        .stream_signature
        .clone();
    std::thread::sleep(KEEPER_SETTLES);

    let new_processor_memory = touched_buffer();
    let unchanged_streams = ResolvedChainAudioConfig {
        inputs: Vec::new(),
        outputs: Vec::new(),
        sample_rate: 48_000.0,
        by_device: HashMap::from([(DeviceId("dev".into()), 48_000.0)]),
        output_devices_by_input_cpal: Vec::new(),
        stream_signature: signature,
    };
    controller
        .upsert_chain_with_resolved(&gain_chain(50.0), unchanged_streams, false)
        .expect("in-place upsert");
    assert!(
        wired_within(&new_processor_memory, AT_ONCE),
        "memory an in-place resync brings in must be wired within 1 s, not at \
         the keeper's next 5 s pass"
    );
}
