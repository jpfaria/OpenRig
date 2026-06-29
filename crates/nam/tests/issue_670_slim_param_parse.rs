//! Issue #670 — unit check: does `plugin_params_from_set_with_defaults`
//! actually map the user-facing `slim` percent into `slim_size`?
use nam::processor::{plugin_params_from_set_with_defaults, DEFAULT_PLUGIN_PARAMS};

#[test]
fn slim_percent_reaches_slim_size() {
    use domain::value_objects::ParameterValue;
    let mut params = block_core::param::ParameterSet::default();
    params.insert("slim", ParameterValue::Float(0.0));
    let parsed =
        plugin_params_from_set_with_defaults(&params, DEFAULT_PLUGIN_PARAMS).expect("parse");
    eprintln!(
        "[#670 PARSE] slim=0.0 (percent) -> slim_size={}",
        parsed.slim_size
    );
    assert_eq!(parsed.slim_size, 0.0, "slim percent must map to ratio");
}
