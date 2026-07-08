//! Regression: audio interface selection resets to empty on reopen.
//!
//! Audio device selection is a SYSTEM concept (ADR 0003) persisted in the
//! per-machine `config.yaml`. On boot the device rows must repopulate their
//! `selected` flag from that global config — independent of the loaded
//! project, which does not (and per the design must not) own the selection.
//! The boot wiring used to feed `build_project_device_rows` an empty slice,
//! so nothing was ever pre-selected and every reopen looked "empty".
//!
//! Also covers Task 13 (#716): device hot-swap keeps bindings in the registry
//! and marks them as unresolved rather than silently dropping them.

use super::*;
use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_filesystem::GuiAudioDeviceSettings;

fn gui_dev(
    id: &str,
    name: &str,
    sample_rate: u32,
    buffer: u32,
    depth: u32,
) -> GuiAudioDeviceSettings {
    GuiAudioDeviceSettings {
        device_id: id.into(),
        name: name.into(),
        sample_rate,
        buffer_size_frames: buffer,
        bit_depth: depth,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

fn descriptor(id: &str, name: &str) -> AudioDeviceDescriptor {
    AudioDeviceDescriptor {
        id: id.into(),
        name: name.into(),
        channels: 2,
    }
}

#[test]
fn boot_preselects_devices_persisted_in_global_config() {
    // Persisted per-machine config from a previous session (config.yaml).
    let saved_inputs = vec![gui_dev("mic", "Mic", 48_000, 64, 24)];
    let saved_outputs = vec![gui_dev("speaker", "Speaker", 48_000, 64, 32)];

    // Live devices enumerated at boot — same ids as what was saved.
    let live_inputs = vec![descriptor("mic", "Mic")];
    let live_outputs = vec![descriptor("speaker", "Speaker")];

    // Boot must derive the pre-fill from the GLOBAL config, not the project.
    let device_settings =
        crate::project_ops::build_device_settings_from_gui(&saved_inputs, &saved_outputs);
    let rows = build_project_device_rows(&live_inputs, &live_outputs, &device_settings);

    assert!(
        rows.iter()
            .any(|r| r.device_id.as_str() == "mic" && r.selected),
        "input device persisted in global config must stay selected on reopen"
    );
    assert!(
        rows.iter()
            .any(|r| r.device_id.as_str() == "speaker" && r.selected),
        "output device persisted in global config must stay selected on reopen"
    );
}

// ── device_refresh_keeps_unresolved_binding ──────────────────────────────────

/// When a device disappears (hot-swap / unplug), bindings that reference its
/// id must NOT be silently dropped. They must remain in the list and be
/// flagged as unresolved so the UI can surface the problem to the user.
#[test]
fn device_refresh_keeps_unresolved_binding() {
    let binding = IoBinding {
        id: "default".to_string(),
        name: "Default".to_string(),
        inputs: vec![IoEndpoint {
            name: "In1".to_string(),
            device_id: DeviceId("devX".to_string()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Out1".to_string(),
            device_id: DeviceId("devX".to_string()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    };
    let bindings = vec![binding];

    // Live devices after refresh — devX is gone.
    let live_inputs = vec![descriptor("devY", "Other Input")];
    let live_outputs = vec![descriptor("devY", "Other Output")];

    let result = check_bindings_after_refresh(&bindings, &live_inputs, &live_outputs);

    // The binding must still exist (not silently dropped).
    assert_eq!(result.len(), 1, "binding must be retained after hot-swap");
    // At least one of its endpoints must be flagged as unresolved.
    assert!(
        result[0].unresolved,
        "binding referencing absent device must be marked unresolved"
    );
    assert_eq!(result[0].binding.id, "default");
}
