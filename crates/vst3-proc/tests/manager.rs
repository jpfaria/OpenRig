//! #251: the manager hands out multiple out-of-process instances of one plugin
//! (one shared child host, one slot per stream) that all process, even while
//! the parent owns an NSApplication.

use std::ffi::c_void;

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
fn sine(frames: usize) -> Vec<[f32; 2]> {
    (0..frames)
        .map(|n| {
            let s = 0.3 * (2.0 * std::f32::consts::PI * 220.0 * n as f32 / SR as f32).sin();
            [s, s]
        })
        .collect()
}

#[test]
fn manager_hands_out_four_processing_instances() {
    vst3_host::init_vst3_catalog(SR);
    let Some(entry) = vst3_host::find_vst3_plugin(MODEL_ID) else {
        eprintln!("ValhallaSupermassive not installed — skipping manager test");
        return;
    };
    let bundle = entry.info.bundle_path.clone();
    let uid = vst3_host::resolve_uid_for_model(MODEL_ID).expect("uid");

    vst3_proc::set_child_bin(env!("CARGO_BIN_EXE_openrig-vst3-proc").into());
    init_nsapp();

    let handles: Vec<_> = (0..4)
        .map(|_| vst3_proc::acquire(&bundle, &uid, SR, 512).expect("acquire instance"))
        .collect();

    for (i, h) in handles.iter().enumerate() {
        let dry = sine(512);
        let mut changed = false;
        for _ in 0..16 {
            let mut buf = dry.clone();
            h.process_block(&mut buf);
            if buf.iter().zip(dry.iter()).any(|(a, b)| (a[0] - b[0]).abs() > 1e-4) {
                changed = true;
            }
        }
        assert!(changed, "manager instance {i} did not process audio");
    }
}
