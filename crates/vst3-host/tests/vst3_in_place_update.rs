//! #251: a VST3 (stereo) block must accept an in-place parameter update, so the
//! engine retunes the LIVE instance on rebuild instead of reloading it. Reloading
//! re-instantiates the plugin, which fails under the app's NSApplication after
//! the first instance — the "VST3 não entra na chain" symptom.
//!
//! Requires ValhallaSupermassive installed. Skips (passes) when absent so CI
//! stays green; run locally where the bundle exists.

use block_core::param::ParameterSet;
use block_core::StereoProcessor;
use domain::value_objects::ParameterValue;
use vst3_host::StereoVst3Processor;

const MODEL_ID: &str = "vst3:ValhallaSupermassive:ValhallaSupermassive";
const SR: f32 = 48_000.0;

#[test]
fn vst3_stereo_processor_updates_params_in_place() {
    vst3_host::init_vst3_catalog(SR as f64, &[]);
    let Some(entry) = vst3_host::find_vst3_plugin(MODEL_ID) else {
        eprintln!("ValhallaSupermassive not installed — skipping in-place update test");
        return;
    };
    let uid = vst3_host::resolve_uid_for_model(MODEL_ID).expect("uid");
    let plugin = vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR as f64, 2, 512, &[])
        .expect("load");
    // VST3 parameter IDs are arbitrary — use the first real one.
    let param_id = plugin.param_info(0).expect("plugin has at least one param").id;
    let mut proc = StereoVst3Processor::new(plugin, None);

    // Set two distinct values in place and confirm the LIVE instance reflects
    // them (round-trips), proving no reload is needed to retune.
    let mut low = ParameterSet::default();
    low.insert(format!("p{param_id}"), ParameterValue::Float(20.0));
    assert!(
        proc.try_in_place_update(&low, SR),
        "a VST3 stereo processor must accept an in-place param update (no reload)"
    );
    let got_low = proc.get_param(param_id);

    let mut high = ParameterSet::default();
    high.insert(format!("p{param_id}"), ParameterValue::Float(80.0));
    assert!(proc.try_in_place_update(&high, SR));
    let got_high = proc.get_param(param_id);

    assert!(
        got_high - got_low > 0.3,
        "in-place update must change the live param (id={param_id}): {got_low} -> {got_high}"
    );
}
