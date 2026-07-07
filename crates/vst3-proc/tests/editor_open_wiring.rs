//! #251: building a VST3 block must register its editor opener under the SAME
//! key the GUI passes to `request_open_editor` — the plugin's model id. If the
//! keys ever diverge, clicking the block silently does nothing (the exact
//! symptom reported). This proves the register ↔ request contract end to end.
//!
//! Requires ValhallaSupermassive installed. Skips (passes) when absent.

use std::ffi::c_void;

use block_core::AudioChannelLayout;

const MODEL_ID: &str = "vst3:ValhallaSupermassive:ValhallaSupermassive";
const SR: f64 = 48_000.0;

#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const i8) -> *mut c_void;
    fn sel_registerName(s: *const i8) -> *mut c_void;
    fn objc_msgSend();
}
fn init_nsapp() {
    unsafe {
        let cls = objc_getClass(b"NSApplication\0".as_ptr() as *const i8);
        let sel = sel_registerName(b"sharedApplication\0".as_ptr() as *const i8);
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        f(cls, sel);
    }
}

#[test]
fn building_a_vst3_block_registers_the_editor_opener_under_its_model() {
    vst3_host::init_vst3_catalog(SR);
    let Some(entry) = vst3_host::find_vst3_plugin(MODEL_ID) else {
        eprintln!("ValhallaSupermassive not installed — skipping");
        return;
    };
    let bundle = entry.info.bundle_path.clone();
    let uid = vst3_host::resolve_uid_for_model(MODEL_ID).expect("uid");
    init_nsapp();

    // Build the block exactly as the engine does. `_proc` MUST stay alive: the
    // opener keeps the mapping via an Arc, but dropping the processor drops the
    // child, and we want to prove the LIVE wiring.
    let _proc = vst3_proc::build_vst3_proc_processor(
        MODEL_ID,
        &bundle,
        &uid,
        SR,
        512,
        AudioChannelLayout::Stereo,
        &[],
    )
    .expect("build out-of-process VST3 processor");

    // The GUI (compact button AND main-screen block click) calls this with the
    // block's model id. It must resolve to the opener we just registered.
    assert!(
        vst3_proc::request_open_editor(MODEL_ID),
        "no editor opener registered for '{MODEL_ID}' after building the block — \
         clicking the block would silently do nothing"
    );

    // And an unrelated key must NOT resolve.
    assert!(!vst3_proc::request_open_editor("vst3:nope:nope"));
}
