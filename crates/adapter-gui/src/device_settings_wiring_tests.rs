use super::*;
use slint::Model;

fn make_item(id: &str) -> DeviceSelectionItem {
    DeviceSelectionItem {
        device_id: id.into(),
        name: id.into(),
        selected: false,
        sample_rate_text: "48000".into(),
        buffer_size_text: "256".into(),
        bit_depth_text: "32".into(),
    }
}

#[test]
fn ctx_construction_clones_share_underlying_model() {
    // Smoke-test that DeviceSettingsCtx can be built and that cloning the
    // Rc-backed VecModel rows propagate to the owner — guarantees that
    // every wired callback observes the same row state via Rc::clone.
    let input = Rc::new(VecModel::from(vec![make_item("in:1")]));
    let output = Rc::new(VecModel::from(vec![make_item("out:1")]));
    let project = Rc::new(VecModel::from(vec![make_item("proj:1")]));

    let ctx = DeviceSettingsCtx {
        input_devices: input.clone(),
        output_devices: output.clone(),
        project_devices: project.clone(),
    };

    // Mutate via clones that the wire() closures would receive…
    toggle_device_row(&ctx.input_devices, 0, true);
    update_device_sample_rate(&ctx.output_devices, 0, "44100".into());
    update_device_buffer_size(&ctx.project_devices, 0, "512".into());
    update_device_bit_depth(&ctx.project_devices, 0, "24".into());

    // …and assert the original Rc owners observe them.
    assert!(input.row_data(0).unwrap().selected);
    assert_eq!(
        output.row_data(0).unwrap().sample_rate_text.as_str(),
        "44100"
    );
    assert_eq!(
        project.row_data(0).unwrap().buffer_size_text.as_str(),
        "512"
    );
    assert_eq!(project.row_data(0).unwrap().bit_depth_text.as_str(), "24");
}
