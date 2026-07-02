//! #251: the out-of-process host must run MULTIPLE VST3 instances even while the
//! PARENT process owns an NSApplication — the exact case that fails in-process.
//! Loads 4 ValhallaSupermassive instances in the child and processes audio
//! through each; every instance must colour the signal (output != dry).
//!
//! Requires ValhallaSupermassive installed. Skips (passes) when absent.

use std::ffi::c_void;

use vst3_proc::Vst3ProcClient;

const MODEL_ID: &str = "vst3:ValhallaSupermassive:ValhallaSupermassive";
const SR: f64 = 48_000.0;

#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const i8) -> *mut c_void;
    fn sel_registerName(s: *const i8) -> *mut c_void;
    fn objc_msgSend();
}

/// Create the process NSApplication, reproducing the app's environment (the
/// thing that makes in-process JUCE instantiation fail after the first).
fn init_nsapp() {
    unsafe {
        let cls = objc_getClass(b"NSApplication\0".as_ptr() as *const i8);
        let sel = sel_registerName(b"sharedApplication\0".as_ptr() as *const i8);
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        f(cls, sel);
    }
}

fn sine(frames: usize) -> Vec<[f32; 2]> {
    (0..frames)
        .map(|n| {
            let s = 0.3 * (2.0 * std::f32::consts::PI * 220.0 * n as f32 / SR as f32).sin();
            [s, s]
        })
        .collect()
}

#[test]
fn four_instances_process_under_parent_nsapp() {
    vst3_host::init_vst3_catalog(SR);
    let Some(entry) = vst3_host::find_vst3_plugin(MODEL_ID) else {
        eprintln!("ValhallaSupermassive not installed — skipping out-of-process test");
        return;
    };
    let bundle = entry.info.bundle_path.clone();
    let uid = vst3_host::resolve_uid_for_model(MODEL_ID).expect("uid");

    // Parent owns an NSApp — in-process this makes instance #2+ fail (#251).
    init_nsapp();

    let child_bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_openrig-vst3-proc"));
    let shm = tempfile::NamedTempFile::new().expect("temp shm");
    let mut client = Vst3ProcClient::spawn(
        &child_bin,
        shm.path(),
        &bundle,
        &uid,
        SR,
        512,
        4,
    )
    .expect("spawn out-of-process host with 4 instances");
    assert_eq!(client.instances(), 4);
    for i in 0..4 {
        client.load_slot(i).expect("load slot");
    }

    // Every instance must process (colour the signal) — none faults.
    for slot in 0..4 {
        let dry = sine(512);
        let mut changed = false;
        // Feed the dry sine block-by-block; the reverb tail builds over blocks.
        for _ in 0..16 {
            let mut buf = dry.clone();
            client.process(slot, &mut buf);
            if buf.iter().zip(dry.iter()).any(|(a, b)| (a[0] - b[0]).abs() > 1e-4) {
                changed = true;
            }
        }
        assert!(
            changed,
            "instance {slot} did not process audio (out-of-process host failed)"
        );
    }
}
