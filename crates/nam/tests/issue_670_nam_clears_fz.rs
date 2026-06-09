//! Issue #670: does the C++ NAM inference clear the thread's flush-to-zero
//! (FZ) FPCR bit? The engine arms FZ once per callback, but if the NAM (which
//! runs before the IR/reverb) clears it, every downstream block runs
//! unprotected and denormal-stalls on a decaying signal — the crackle the
//! user pinned to the IR (disabling it breaks the stalled cascade).
#![cfg(all(target_arch = "aarch64", not(debug_assertions)))]

use block_core::MonoProcessor;
use nam::processor::{NamProcessor, DEFAULT_PLUGIN_PARAMS};

fn fixture_model() -> String {
    format!(
        "{}/../engine/tests/fixtures/plugins/nam/marshall_plexi/captures/angus_nano.nam",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn fz_bit() -> u64 {
    let fpcr: u64;
    unsafe { core::arch::asm!("mrs {}, fpcr", out(reg) fpcr) };
    (fpcr >> 24) & 1
}

unsafe fn arm_fz() {
    let mut fpcr: u64;
    core::arch::asm!("mrs {}, fpcr", out(reg) fpcr);
    core::arch::asm!("msr fpcr, {}", in(reg) fpcr | (1 << 24));
}

#[test]
fn nam_inference_does_not_clear_flush_to_zero() {
    let mut params = DEFAULT_PLUGIN_PARAMS;
    params.noise_gate_enabled = false;
    let mut proc = NamProcessor::new(&fixture_model(), None, params, 48_000.0)
        .expect("fixture NAM must load");

    unsafe { arm_fz() };
    assert_eq!(fz_bit(), 1, "FZ must be armed before the NAM call");

    let mut buf = vec![0.1f32; 64];
    proc.process_block(&mut buf);

    let after = fz_bit();
    assert_eq!(
        after, 1,
        "BUG #670: the NAM inference CLEARED the flush-to-zero bit (FZ={after} \
         after the call). Every block after the NAM (IR, reverb) then runs \
         denormal-unprotected and stalls on a decaying signal. The engine \
         must re-arm FZ before each block (or the NAM must restore it)."
    );
}
