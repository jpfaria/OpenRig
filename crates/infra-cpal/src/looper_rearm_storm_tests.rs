//! #903 — a steady loop must not respawn its render on every tick.
//!
//! `sync_looper_streams` runs on the meter tick and re-arms a loop whose
//! CONTENT moved. If anything in that key changes on its own, a playing loop
//! spawns a fresh render thread every tick: each one builds the chain again
//! (NAM + IR off disk) and burns a core, they pile up, and the machine — GUI
//! and audio together — goes down with them. The owner's "algo consumindo e
//! empilhando; o áudio fica lento".

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::controller_live_edit_replicates_user_report_tests::{
    controller_with_di_only_chain, gain_chain, init_registry,
};
use super::controller_loopers::LOOPER_ARMS;

#[test]
fn two_playing_loops_keep_their_renders_across_many_ticks() {
    init_registry();
    let mut chain = gain_chain(100.0);
    chain.loopers = vec![
        project::chain::LooperConfig::new(1),
        project::chain::LooperConfig::new(2),
    ];
    let controller = controller_with_di_only_chain(&chain);

    // Two recorded loops, both playing — what the chain-wide transport leaves
    // behind, and what the owner had on screen when it seized up.
    controller.looper_load(&chain.id, 1, &vec![0.3f32; 4_800 * 2]);
    controller.looper_load(&chain.id, 2, &vec![0.3f32; 4_800 * 2]);
    controller.looper_play(&chain.id, 1);

    controller.sync_looper_streams(&chain);
    std::thread::sleep(Duration::from_millis(200));
    let after_first = LOOPER_ARMS.load(Ordering::Relaxed);

    // The meter tick, over and over, with nothing changing.
    for _ in 0..20 {
        controller.sync_looper_streams(&chain);
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(200));
    let after_many = LOOPER_ARMS.load(Ordering::Relaxed);

    assert_eq!(
        after_many, after_first,
        "a steady loop must keep the render it already has — arms went from \
         {after_first} to {after_many} over 20 idle ticks, and each arm rebuilds the \
         chain, copies the take and spawns a render thread"
    );
    let _ = Arc::new(());
}
