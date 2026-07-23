//! Issue #627 (second symptom) — changing a device's buffer size via
//! Settings → Audio → Aplicar must survive a subsequent **whole-config**
//! re-save of the in-memory `AppConfig` (which happens on every
//! project-open / register-recent lifecycle event).
//!
//! User reproduction:
//!   1. Settings → Audio → Aplicar with buffer size 128.
//!   2. Open / switch project (any lifecycle event that re-persists the
//!      in-memory app config via `save_app_config(&app_config.borrow())`).
//!   3. `config.yaml` `input_devices[0].buffer_size_frames` is back to 64.
//!
//! Root cause (same class as #607 — path overrides): `Command::SaveAudioSettings`
//! persists to disk but does NOT update the shared in-memory `AppConfig`.
//! The next whole-config save writes the stale snapshot, reverting the
//! buffer the user just applied.
//!
//! Fix contract: applying audio settings must update the in-memory `AppConfig`
//! in lockstep with the disk write, so a later whole-config save carries the
//! user's choice. This test exercises that seam (`apply_audio_override`) and
//! is RED today (the function does not exist yet).

use std::path::PathBuf;
use std::sync::Mutex;

use adapter_gui::apply_audio_override;
use infra_filesystem::{FilesystemStorage, GuiAudioDeviceSettings};

/// HOME is process-global; serialize tests that swap it.
static HOME_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home<F: FnOnce(&PathBuf)>(label: &str, f: F) {
    let _g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp =
        std::env::temp_dir().join(format!("openrig-627-{label}-{}-{now}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("mkdir tempdir");
    let prev = std::env::var_os("HOME");
    // dirs::config_dir() honours $XDG_CONFIG_HOME over $HOME/.config on Linux
    // (CI runners set it), so a HOME-only swap leaks to the runner's real
    // config dir. Track XDG alongside HOME so config paths follow the tempdir.
    let prev_xdg = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("HOME", &tmp);
    std::env::set_var("XDG_CONFIG_HOME", tmp.join(".config"));
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&tmp)));
    if let Some(prev) = prev {
        std::env::set_var("HOME", prev);
    } else {
        std::env::remove_var("HOME");
    }
    if let Some(prev_xdg) = prev_xdg {
        std::env::set_var("XDG_CONFIG_HOME", prev_xdg);
    } else {
        std::env::remove_var("XDG_CONFIG_HOME");
    }
    let _ = std::fs::remove_dir_all(&tmp);
    if let Err(p) = res {
        std::panic::resume_unwind(p);
    }
}

fn device_with_buffer(id: &str, buffer: u32) -> GuiAudioDeviceSettings {
    GuiAudioDeviceSettings {
        device_id: id.into(),
        name: id.into(),
        sample_rate: 48_000,
        buffer_size_frames: buffer,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

#[test]
fn issue_627_buffer_override_survives_whole_config_resave() {
    with_temp_home("buf-clobber", |_| {
        // --- Step 1: Aplicar writes buffer 128 to disk (simulated).
        // Build the device list as Aplicar would dispatch it.
        let input_128 = vec![device_with_buffer("built-in-mic", 128)];
        let output_128 = vec![device_with_buffer("built-in-out", 128)];

        // Persist to disk exactly as SaveAudioSettings handler does.
        {
            let mut config_on_disk =
                FilesystemStorage::load_app_config().expect("initial disk load");
            config_on_disk.input_devices = input_128.clone();
            config_on_disk.output_devices = output_128.clone();
            FilesystemStorage::save_app_config(&config_on_disk).expect("persist buffer 128");
        }

        // Confirm disk shows 128.
        let from_disk = FilesystemStorage::load_app_config().expect("reload after Aplicar");
        assert_eq!(
            from_disk.input_devices[0].buffer_size_frames, 128,
            "disk must hold buffer 128 after Aplicar"
        );

        // --- Step 2: The GUI's in-memory snapshot is still at startup value (64).
        let mut in_memory = FilesystemStorage::load_app_config().expect("fresh in-memory load");
        // Simulate stale: the in-memory view still shows buffer 64.
        in_memory.input_devices = vec![device_with_buffer("built-in-mic", 64)];
        in_memory.output_devices = vec![device_with_buffer("built-in-out", 64)];
        assert_eq!(
            in_memory.input_devices[0].buffer_size_frames, 64,
            "in-memory snapshot starts stale at 64"
        );

        // --- Step 3: apply_audio_override must sync the in-memory snapshot.
        apply_audio_override(&mut in_memory, &input_128, &output_128);

        assert_eq!(
            in_memory.input_devices[0].buffer_size_frames, 128,
            "apply_audio_override must mirror the applied buffer into the in-memory AppConfig"
        );
        assert_eq!(
            in_memory.output_devices[0].buffer_size_frames, 128,
            "apply_audio_override must mirror output devices too"
        );

        // --- Step 4: lifecycle whole-config re-save uses the (now-synced) snapshot.
        FilesystemStorage::save_app_config(&in_memory).expect("whole-config re-save");

        // --- Step 5: reload and assert buffer survived.
        let reloaded = FilesystemStorage::load_app_config().expect("final reload");
        assert_eq!(
            reloaded.input_devices[0].buffer_size_frames, 128,
            "REGRESSION #627: whole-config re-save clobbered the user's buffer 128 back to 64"
        );
        assert_eq!(
            reloaded.output_devices[0].buffer_size_frames, 128,
            "REGRESSION #627: output buffer clobbered back to 64"
        );
    });
}
