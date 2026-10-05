//! #979 — the backing-track player, the metronome and the drums opened their
//! own output on the HD 8 asking for 512 frames while the chain ran at 64.
//! On CoreAudio the buffer size is a device property per process, so the
//! auxiliary stream re-sized the device under the live chain: measured on the
//! rig, the device went to 512 frames, the chain's callbacks fell from ~689/s
//! to ~86/s and ~75% of its frames were dropped until the chain was restarted.
//! An auxiliary stream must open its device with the project's settings for it,
//! exactly as the chain's own streams do.

use domain::ids::DeviceId;
use project::device::DeviceSettings;

use crate::aux_stream_format::aux_stream_format;

const HD8: &str = "coreaudio:TUSBAudio:Fender:Quantum HD 8";
const OTHER: &str = "coreaudio:BlackHole2ch";

fn settings(device_id: &str, sample_rate: u32, buffer_size_frames: u32) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(device_id.into()),
        sample_rate,
        buffer_size_frames,
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
fn aux_stream_opens_its_device_with_the_project_buffer_and_rate() {
    let project = vec![settings(OTHER, 48_000, 512), settings(HD8, 44_100, 64)];

    let format = aux_stream_format(&project, HD8, 48_000);

    assert_eq!(
        format.buffer_frames, 64,
        "the chain on this device runs at 64 frames"
    );
    assert_eq!(
        format.sample_rate, 44_100,
        "the chain on this device runs at 44.1 kHz"
    );
}

#[test]
fn aux_stream_on_a_device_the_project_does_not_configure_matches_a_chain_stream() {
    let project = vec![settings(HD8, 44_100, 64)];

    let format = aux_stream_format(&project, OTHER, 48_000);

    assert_eq!(
        format.sample_rate, 48_000,
        "no project rate: the device keeps its own"
    );
    assert_eq!(
        format.buffer_frames,
        crate::stream_rates::UNSET_BUFFER_SIZE_FRAMES,
        "no project buffer: the same fallback a chain stream uses"
    );
}
