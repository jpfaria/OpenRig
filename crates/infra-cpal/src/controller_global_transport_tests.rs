//! Shares the live-controller fixtures of
//! `controller_live_edit_replicates_user_report_tests`, which the linux+jack
//! build cfg-s out (#755) — so this file is gated the same way.

#![cfg(not(all(target_os = "linux", feature = "jack")))]

//! #903 — the controller's side of the global transport and the live level.

use super::controller_live_edit_replicates_user_report_tests::{
    controller_with_di_only_chain, gain_chain, init_registry,
};
use engine::LooperState;

fn take() -> Vec<f32> {
    vec![0.4f32; 4_800 * 2]
}

#[test]
fn the_controllers_global_transport_moves_every_loop_it_holds() {
    init_registry();
    let chain = gain_chain(100.0);
    let controller = controller_with_di_only_chain(&chain);
    for uid in [1u64, 2] {
        controller.looper_load(&chain.id, uid, &take());
    }

    controller.looper_play_all(&chain.id);
    let playing: Vec<LooperState> = [1u64, 2]
        .iter()
        .map(|uid| {
            controller
                .chain_looper_status(&chain.id, *uid)
                .unwrap()
                .state
        })
        .collect();

    controller.looper_stop_all(&chain.id);
    let stopped: Vec<LooperState> = [1u64, 2]
        .iter()
        .map(|uid| {
            controller
                .chain_looper_status(&chain.id, *uid)
                .unwrap()
                .state
        })
        .collect();

    assert_eq!(playing, vec![LooperState::Playing, LooperState::Playing]);
    assert_eq!(stopped, vec![LooperState::Stopped, LooperState::Stopped]);
}

/// The level reaches the store and the playback door without a re-arm — with
/// nothing armed the push is a no-op, which is exactly what it must be.
#[test]
fn setting_the_level_pushes_it_without_arming_anything() {
    init_registry();
    let chain = gain_chain(100.0);
    let controller = controller_with_di_only_chain(&chain);
    controller.looper_load(&chain.id, 1, &take());
    let content_before = controller
        .chain_looper_status(&chain.id, 1)
        .unwrap()
        .content_rev;

    controller.looper_set_mix(&chain.id, 1, 0.3);

    assert_eq!(
        controller
            .chain_looper_status(&chain.id, 1)
            .unwrap()
            .content_rev,
        content_before,
        "the level is not content — it must not ask for a re-render"
    );
}
