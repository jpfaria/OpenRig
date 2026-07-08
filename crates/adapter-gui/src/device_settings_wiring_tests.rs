use super::*;
use application::command::Command;
use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
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

// ── wizard_finish_creates_default_binding ────────────────────────────────────

/// Finishing the audio wizard with a chosen input + output device dispatches
/// `Command::CreateIoBinding` with id == "default" and the expected endpoints.
#[test]
fn wizard_finish_creates_default_binding() {
    let cmd = wizard_create_or_update_default_binding(
        "devA", "devA", None, // no existing default binding → CREATE
    );

    match cmd {
        Command::CreateIoBinding { binding } => {
            assert_eq!(binding.id, "default", "binding id must be 'default'");
            assert!(
                !binding.inputs.is_empty(),
                "must have at least one input endpoint"
            );
            assert!(
                !binding.outputs.is_empty(),
                "must have at least one output endpoint"
            );
            // Input endpoint uses the chosen input device
            assert_eq!(
                binding.inputs[0].device_id,
                DeviceId("devA".to_string()),
                "input endpoint must reference the chosen input device"
            );
            // Output endpoint uses the chosen output device
            assert_eq!(
                binding.outputs[0].device_id,
                DeviceId("devA".to_string()),
                "output endpoint must reference the chosen output device"
            );
            // Input defaults to Mono (single guitar channel)
            assert_eq!(binding.inputs[0].mode, ChannelMode::Mono);
            // Output defaults to Stereo
            assert_eq!(binding.outputs[0].mode, ChannelMode::Stereo);
        }
        other => panic!("expected CreateIoBinding, got {other:?}"),
    }
}

// ── wizard_finish_updates_existing_default ───────────────────────────────────

/// If a "default" binding already exists, finishing the wizard emits
/// `Command::UpdateIoBinding` (not a duplicate create).
#[test]
fn wizard_finish_updates_existing_default() {
    let existing = IoBinding {
        id: "default".to_string(),
        name: "Default".to_string(),
        inputs: vec![IoEndpoint {
            name: "In1".to_string(),
            device_id: DeviceId("old-device".to_string()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Out1".to_string(),
            device_id: DeviceId("old-device".to_string()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    };

    let cmd =
        wizard_create_or_update_default_binding("new-input-dev", "new-output-dev", Some(&existing));

    match cmd {
        Command::UpdateIoBinding { binding } => {
            assert_eq!(binding.id, "default");
            assert_eq!(
                binding.inputs[0].device_id,
                DeviceId("new-input-dev".to_string()),
                "input endpoint must be updated to new device"
            );
            assert_eq!(
                binding.outputs[0].device_id,
                DeviceId("new-output-dev".to_string()),
                "output endpoint must be updated to new device"
            );
        }
        other => panic!("expected UpdateIoBinding, got {other:?}"),
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
