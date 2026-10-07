use super::*;

#[test]
fn every_engine_control_of_a_capture_plugin_is_named() {
    let names = capture_engine_parameter_names();
    let engine = nam::processor::plugin_parameter_specs()
        .into_iter()
        .chain([nam::processor::slim_parameter_spec()])
        .chain(block_reverb::ir_reverb_parameter_specs())
        .map(|spec| spec.path);
    for path in engine {
        assert!(names.contains(&path), "{path} missing from {names:?}");
    }
    assert!(names.contains(&"output_db".to_string()));
}
