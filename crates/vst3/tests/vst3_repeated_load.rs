//! Regression guard for #251: loading the same VST3 plugin repeatedly in one
//! process — sequentially, concurrently, and after creating an editor view —
//! must keep succeeding. It documents what is NOT the cause of the
//! "load once, close, can't reopen" report: the raw load / createView path is
//! fine; the breakage only appears once a view is attached to a real window and
//! that window is closed (an AppKit lifecycle that can't run headless here).
//!
//! Requires ValhallaSupermassive installed. Skips (passes) when absent so CI
//! without the plugin stays green; run locally where the bundle exists.

use vst3::ComPtr;
use vst3::Steinberg::IPlugView;
use vst3::Steinberg::Vst::{IConnectionPoint, IConnectionPointTrait, IEditControllerTrait, ViewType};

const MODEL_ID: &str = "vst3:ValhallaSupermassive:ValhallaSupermassive";

fn load(bundle: &std::path::Path, uid: &[u8; 16], sr: f64) -> anyhow::Result<vst3_host::Vst3Plugin> {
    vst3_host::Vst3Plugin::load(bundle, uid, sr, 2, 512, &[])
}

#[test]
fn repeated_in_process_load_succeeds() {
    let sr = 48_000.0_f64;
    vst3_host::init_vst3_catalog(sr);

    let Some(entry) = vst3_host::find_vst3_plugin(MODEL_ID) else {
        eprintln!("ValhallaSupermassive not installed — skipping repeated-load guard");
        return;
    };
    let bundle = entry.info.bundle_path.clone();
    let uid = vst3_host::resolve_uid_for_model(MODEL_ID).expect("resolve uid");

    // Sequential: load, drop, load again.
    let p = load(&bundle, &uid, sr).expect("1st sequential load");
    drop(p);
    let p = load(&bundle, &uid, sr).expect("2nd sequential load after drop");
    drop(p);

    // Concurrent: two instances alive at once.
    let a = load(&bundle, &uid, sr).expect("concurrent load #1");
    let b = load(&bundle, &uid, sr).expect("concurrent load #2 while #1 alive");
    drop(a);
    drop(b);

    // After creating (and leaking, without removed()) an editor view.
    let p1 = load(&bundle, &uid, sr).expect("load before createView");
    let controller = p1.controller_clone();
    unsafe {
        if let Some(comp_cp) = p1.component().cast::<IConnectionPoint>() {
            if let Some(ctrl_cp) = controller.cast::<IConnectionPoint>() {
                let _ = comp_cp.connect(ctrl_cp.as_ptr());
                let _ = ctrl_cp.connect(comp_cp.as_ptr());
            }
        }
    }
    let view_ptr = unsafe { controller.createView(ViewType::kEditor) };
    assert!(!view_ptr.is_null(), "createView returned null");
    let _leaked_view: ComPtr<IPlugView> = unsafe { ComPtr::from_raw_unchecked(view_ptr) };
    let p2 = load(&bundle, &uid, sr);
    assert!(
        p2.is_ok(),
        "load after an orphaned (never-removed) editor view must still succeed, got: {:?}",
        p2.err()
    );
}
