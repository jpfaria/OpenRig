//! #903 — a loop's render thread must not compete with the live callback.
//!
//! `CLAUDE.md` LAW: streams are independent, and independence includes CPU
//! TIME. The isolated playback render (a loop, a DI) promoted itself to the
//! SAME Mach time-constraint class the chain's audio callback runs in, so the
//! two saw each other through the scheduler: with a loop playing, the live
//! guitar's latency climbed, and it climbed again with every loop stacked on
//! top — the owner's "empilhando loop vai aumentando a latência". Ten guitars
//! means ten pipelines that do not know the others exist.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::di_stream_worker::{RenderScheduling, LAST_RENDER_SCHEDULING};

#[test]
fn a_loops_render_thread_never_takes_the_live_callbacks_scheduling_class() {
    super::controller_live_edit_replicates_user_report_tests::init_registry();
    let chain = super::controller_live_edit_replicates_user_report_tests::gain_chain(100.0);
    let mut controller =
        super::controller_live_edit_replicates_user_report_tests::controller_with_di_only_chain(
            &chain,
        );
    LAST_RENDER_SCHEDULING.store(RenderScheduling::Unset as u8, Ordering::Relaxed);

    let pcm = Arc::new(engine::DiPcm::new(vec![0.6; 48_000], 48_000, 1));
    controller.arm_di_stream(&chain, pcm).expect("arm the loop");
    // The render thread declares its policy as it starts.
    for _ in 0..100 {
        if LAST_RENDER_SCHEDULING.load(Ordering::Relaxed) != RenderScheduling::Unset as u8 {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    let applied = LAST_RENDER_SCHEDULING.load(Ordering::Relaxed);
    assert_ne!(
        applied,
        RenderScheduling::Unset as u8,
        "precondition: the render thread must have declared a scheduling class"
    );
    assert_ne!(
        applied,
        RenderScheduling::AudioRealtime as u8,
        "a loop's render must never share the live callback's time-constraint \
         class — that is how one stream reaches the other's clock and adds \
         latency to it (CLAUDE.md: streams are independent, CPU time included)"
    );
}
