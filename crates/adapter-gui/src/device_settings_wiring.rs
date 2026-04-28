//! Wiring for the trivial device settings toggles and value updates.
//!
//! Registers the small one-liner callbacks that delegate to `audio_devices`
//! helpers: device toggle, sample rate, buffer size, bit depth — for both
//! the main `AppWindow` (input/output/project) and the standalone
//! `ProjectSettingsWindow`. Lives outside `lib.rs` so settings UI work can
//! evolve without colliding with other features in parallel branches.
//!
//! The bigger settings callbacks (`on_save_audio_settings`, `on_refresh_devices`,
//! the audio-step navigation) stay in `lib.rs` for now — they capture broader
//! project state and will move in a follow-up slice.

use std::rc::Rc;

use slint::VecModel;

use crate::audio_devices::{
    toggle_device_row, update_device_bit_depth, update_device_buffer_size,
    update_device_sample_rate,
};
use crate::{AppWindow, DeviceSelectionItem, ProjectSettingsWindow};

/// Models backing the device selection lists shared by the main window and the
/// project settings window. Each `Rc` is cloned per closure that needs it.
pub(crate) struct DeviceSettingsCtx {
    pub input_devices: Rc<VecModel<DeviceSelectionItem>>,
    pub output_devices: Rc<VecModel<DeviceSelectionItem>>,
    pub project_devices: Rc<VecModel<DeviceSelectionItem>>,
}

pub(crate) fn wire(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    ctx: DeviceSettingsCtx,
) {
    let DeviceSettingsCtx {
        input_devices,
        output_devices,
        project_devices,
    } = ctx;

    // Input device toggles & value updates (main window).
    {
        let input_devices = input_devices.clone();
        window.on_toggle_input_device(move |index, selected| {
            toggle_device_row(&input_devices, index as usize, selected);
        });
    }
    {
        let input_devices = input_devices.clone();
        window.on_update_input_sample_rate(move |index, value| {
            update_device_sample_rate(&input_devices, index as usize, value);
        });
    }
    {
        let input_devices = input_devices.clone();
        window.on_update_input_buffer_size(move |index, value| {
            update_device_buffer_size(&input_devices, index as usize, value);
        });
    }

    // Output device toggles & value updates (main window).
    {
        let output_devices = output_devices.clone();
        window.on_toggle_output_device(move |index, selected| {
            toggle_device_row(&output_devices, index as usize, selected);
        });
    }
    {
        let output_devices = output_devices.clone();
        window.on_update_output_sample_rate(move |index, value| {
            update_device_sample_rate(&output_devices, index as usize, value);
        });
    }
    {
        let output_devices = output_devices.clone();
        window.on_update_output_buffer_size(move |index, value| {
            update_device_buffer_size(&output_devices, index as usize, value);
        });
    }

    // Project device toggles & value updates — standalone ProjectSettingsWindow.
    {
        let project_devices = project_devices.clone();
        project_settings_window.on_toggle_project_device(move |index, selected| {
            toggle_device_row(&project_devices, index as usize, selected);
        });
    }
    {
        let project_devices = project_devices.clone();
        project_settings_window.on_update_project_sample_rate(move |index, value| {
            update_device_sample_rate(&project_devices, index as usize, value);
        });
    }
    {
        let project_devices = project_devices.clone();
        project_settings_window.on_update_project_buffer_size(move |index, value| {
            update_device_buffer_size(&project_devices, index as usize, value);
        });
    }
    {
        let project_devices = project_devices.clone();
        project_settings_window.on_update_project_bit_depth(move |index, value| {
            update_device_bit_depth(&project_devices, index as usize, value);
        });
    }

    // Project device toggles & value updates — fullscreen inline project
    // settings on the main window (mirrors the standalone settings window).
    {
        let project_devices = project_devices.clone();
        window.on_toggle_project_device(move |index, selected| {
            toggle_device_row(&project_devices, index as usize, selected);
        });
    }
    {
        let project_devices = project_devices.clone();
        window.on_update_project_sample_rate(move |index, value| {
            update_device_sample_rate(&project_devices, index as usize, value);
        });
    }
    {
        let project_devices = project_devices.clone();
        window.on_update_project_buffer_size(move |index, value| {
            update_device_buffer_size(&project_devices, index as usize, value);
        });
    }
    {
        let project_devices = project_devices.clone();
        window.on_update_project_bit_depth(move |index, value| {
            update_device_bit_depth(&project_devices, index as usize, value);
        });
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(output.row_data(0).unwrap().sample_rate_text.as_str(), "44100");
        assert_eq!(project.row_data(0).unwrap().buffer_size_text.as_str(), "512");
        assert_eq!(project.row_data(0).unwrap().bit_depth_text.as_str(), "24");
    }
}
