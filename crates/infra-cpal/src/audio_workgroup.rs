//! Issue #670 — join each audio callback thread to its device's OS workgroup
//! (macOS). The chain DSP runs in the cpal INPUT callback and the mix+limiter
//! in the OUTPUT callback; when the OS lets either thread drift off the audio
//! I/O core cluster, its cache — notably the ~290 KB of NAM A2 weights — is
//! lost to the UI thread between buffers and the next buffer reloads cold,
//! spiking a 64-frame buffer past its deadline (the xrun / "crackle").
//!
//! Joining the device `os_workgroup` co-schedules the thread with that
//! device's real-time work and keeps it cache-warm. Unlike a
//! `THREAD_TIME_CONSTRAINT_POLICY` promotion it reserves NO real-time band, so
//! it cannot oversubscribe when several chains each run their own callback
//! thread (that oversubscription is what made the earlier RT promotion worse).
//!
//! Best-effort and idempotent per thread: every failure path is a silent
//! no-op, never worse than not joining. macOS only; a no-op elsewhere.

#[cfg(target_os = "macos")]
mod imp {
    use std::cell::Cell;
    use std::os::raw::c_void;

    #[repr(C)]
    struct AudioObjectPropertyAddress {
        selector: u32,
        scope: u32,
        element: u32,
    }

    /// `os_workgroup_t` — an opaque OS object pointer.
    type OsWorkgroup = *mut c_void;

    /// `os_workgroup_join_token_s` is 4× u64; oversize here so a join can never
    /// write past the allocation even if the SDK layout grows.
    #[repr(C)]
    struct JoinToken {
        _opaque: [u64; 8],
    }

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyData(
            object_id: u32,
            address: *const AudioObjectPropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            data_size: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }

    extern "C" {
        fn os_workgroup_join(workgroup: OsWorkgroup, token: *mut JoinToken) -> i32;
    }

    const SYSTEM_OBJECT: u32 = 1; // kAudioObjectSystemObject
    const DEFAULT_INPUT_DEVICE: u32 = u32::from_be_bytes(*b"dIn "); // kAudioHardwarePropertyDefaultInputDevice
    const DEFAULT_OUTPUT_DEVICE: u32 = u32::from_be_bytes(*b"dOut"); // kAudioHardwarePropertyDefaultOutputDevice
    const IO_WORKGROUP: u32 = u32::from_be_bytes(*b"oswg"); // kAudioDevicePropertyIOThreadOSWorkgroup
    const SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob"); // kAudioObjectPropertyScopeGlobal

    fn property_u32(object: u32, selector: u32) -> Option<u32> {
        let address = AudioObjectPropertyAddress {
            selector,
            scope: SCOPE_GLOBAL,
            element: 0, // kAudioObjectPropertyElementMain
        };
        let mut value: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                &mut value as *mut u32 as *mut c_void,
            )
        };
        (status == 0 && value != 0).then_some(value)
    }

    fn device_workgroup(device: u32) -> Option<OsWorkgroup> {
        let address = AudioObjectPropertyAddress {
            selector: IO_WORKGROUP,
            scope: SCOPE_GLOBAL,
            element: 0,
        };
        let mut workgroup: OsWorkgroup = std::ptr::null_mut();
        let mut size = std::mem::size_of::<OsWorkgroup>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                device,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                &mut workgroup as *mut OsWorkgroup as *mut c_void,
            )
        };
        (status == 0 && !workgroup.is_null()).then_some(workgroup)
    }

    const HW_DEVICES: u32 = u32::from_be_bytes(*b"dev#"); // kAudioHardwarePropertyDevices
    const DEVICE_UID: u32 = u32::from_be_bytes(*b"uid "); // kAudioDevicePropertyDeviceUID
    const UTF8: u32 = 0x0800_0100; // kCFStringEncodingUTF8

    #[link(name = "CoreAudio", kind = "framework")]
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyDataSize(
            object: u32,
            address: *const AudioObjectPropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
        ) -> i32;
        fn CFStringGetCString(
            the_string: *const c_void,
            buffer: *mut core::ffi::c_char,
            buffer_size: isize,
            encoding: u32,
        ) -> u8;
        fn CFStringGetLength(the_string: *const c_void) -> isize;
        fn CFRelease(cf: *const c_void);
    }

    /// Every audio device's `AudioObjectID`.
    fn all_audio_device_ids() -> Vec<u32> {
        let address = AudioObjectPropertyAddress {
            selector: HW_DEVICES,
            scope: SCOPE_GLOBAL,
            element: 0,
        };
        let mut size: u32 = 0;
        let st = unsafe {
            AudioObjectGetPropertyDataSize(SYSTEM_OBJECT, &address, 0, std::ptr::null(), &mut size)
        };
        if st != 0 || size == 0 {
            return Vec::new();
        }
        let count = size as usize / std::mem::size_of::<u32>();
        let mut ids = vec![0u32; count];
        let mut sz = size;
        let st = unsafe {
            AudioObjectGetPropertyData(
                SYSTEM_OBJECT,
                &address,
                0,
                std::ptr::null(),
                &mut sz,
                ids.as_mut_ptr() as *mut c_void,
            )
        };
        if st != 0 {
            return Vec::new();
        }
        ids
    }

    /// The CoreAudio `kAudioDevicePropertyDeviceUID` of a device — the same
    /// string cpal exposes as the tail of its `coreaudio:<uid>` device id.
    fn device_uid(device: u32) -> Option<String> {
        let address = AudioObjectPropertyAddress {
            selector: DEVICE_UID,
            scope: SCOPE_GLOBAL,
            element: 0,
        };
        let mut cfstr: *const c_void = std::ptr::null();
        let mut size = std::mem::size_of::<*const c_void>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                device,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                &mut cfstr as *mut *const c_void as *mut c_void,
            )
        };
        if status != 0 || cfstr.is_null() {
            return None;
        }
        // kAudioDevicePropertyDeviceUID returns a +1 retained CFString; release it.
        let len = unsafe { CFStringGetLength(cfstr) };
        let cap = (len.max(0) as usize) * 4 + 1; // UTF-8 worst case + NUL
        let mut buf = vec![0 as core::ffi::c_char; cap];
        let ok = unsafe { CFStringGetCString(cfstr, buf.as_mut_ptr(), cap as isize, UTF8) };
        unsafe { CFRelease(cfstr) };
        if ok == 0 {
            return None;
        }
        unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
            .to_str()
            .ok()
            .map(|s| s.to_string())
    }

    /// The `AudioObjectID` whose UID matches a cpal `coreaudio:<uid>` device id.
    fn device_id_for_uid(bound: &str) -> Option<u32> {
        let uid = bound.strip_prefix("coreaudio:").unwrap_or(bound);
        all_audio_device_ids()
            .into_iter()
            .find(|&d| device_uid(d).as_deref() == Some(uid))
    }

    /// Join the current thread to the OS workgroup of the device the callback
    /// actually serves (`bound`), falling back to `default_selector` only when
    /// no bound device is known. `attempted` is a per-thread guard so it runs
    /// at most once (#760).
    fn join(bound: Option<&str>, default_selector: u32, label: &str, attempted: &Cell<bool>) {
        if attempted.get() {
            return;
        }
        attempted.set(true); // try at most once per thread, whatever happens

        let device = match super::workgroup_join_target(bound) {
            super::WorkgroupTarget::Device(uid) => match device_id_for_uid(&uid) {
                Some(d) => d,
                None => {
                    log::warn!(
                        "[#760] workgroup: no CoreAudio device matches bound uid '{uid}' — \
                         {label} thread runs un-coscheduled (skipping join)"
                    );
                    return;
                }
            },
            super::WorkgroupTarget::SystemDefault => {
                match property_u32(SYSTEM_OBJECT, default_selector) {
                    Some(d) => d,
                    None => {
                        log::warn!("[#670] workgroup: no default {label} device (skipping join)");
                        return;
                    }
                }
            }
        };
        let Some(workgroup) = device_workgroup(device) else {
            log::warn!("[#670] workgroup: {label} device {device} has no OS workgroup (skipping)");
            return;
        };
        // The thread never leaves the workgroup, so the join token must outlive
        // it — leak it (one tiny allocation per audio thread).
        let token = Box::leak(Box::new(JoinToken { _opaque: [0; 8] }));
        let rc = unsafe { os_workgroup_join(workgroup, token) };
        if rc == 0 {
            log::info!("[#670] audio {label} callback thread joined the device OS workgroup");
        } else {
            log::info!(
                "[#670] workgroup: {label} os_workgroup_join returned {rc} \
                 (EALREADY/already-a-member is harmless)"
            );
        }
    }

    thread_local! {
        static INPUT_ATTEMPTED: Cell<bool> = const { Cell::new(false) };
        static OUTPUT_ATTEMPTED: Cell<bool> = const { Cell::new(false) };
    }

    pub(crate) fn ensure_joined_input(bound: Option<&str>) {
        INPUT_ATTEMPTED.with(|a| join(bound, DEFAULT_INPUT_DEVICE, "input", a));
    }

    pub(crate) fn ensure_joined_output(bound: Option<&str>) {
        OUTPUT_ATTEMPTED.with(|a| join(bound, DEFAULT_OUTPUT_DEVICE, "output", a));
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub(crate) fn ensure_joined_input(_bound: Option<&str>) {}
    pub(crate) fn ensure_joined_output(_bound: Option<&str>) {}
}

/// Join the current INPUT-callback thread to the input device's OS workgroup
/// once. Cheap thread-local check after the first call. See module docs.
/// Which device's OS workgroup a callback thread should join.
///
/// The audio thread must be co-scheduled with the IO thread of the device it
/// actually serves. In a multi-device rig that is the device the stream is
/// bound to — NOT the system default (#760).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // consumed by join() on macOS + tests
pub(crate) enum WorkgroupTarget {
    /// Join the workgroup of the device with this id (the bound device).
    Device(String),
    /// No bound device is known — fall back to the system default device.
    SystemDefault,
}

/// Decide which device's workgroup a callback bound to `bound_device` joins.
///
/// The callback thread must co-schedule with the IO thread of the device it
/// actually serves — the bound device. Only when no bound device is known (a
/// legacy single-device caller) may it fall back to the system default.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // join() (macOS) + tests
pub(crate) fn workgroup_join_target(bound_device: Option<&str>) -> WorkgroupTarget {
    match bound_device {
        Some(id) if !id.is_empty() => WorkgroupTarget::Device(id.to_string()),
        _ => WorkgroupTarget::SystemDefault,
    }
}

#[cfg(test)]
#[path = "audio_workgroup_tests.rs"]
mod audio_workgroup_tests;

pub(crate) fn ensure_joined_input(bound: Option<&str>) {
    imp::ensure_joined_input(bound);
}

/// Join the current OUTPUT-callback thread to the output device's OS workgroup
/// once. Cheap thread-local check after the first call. See module docs.
pub(crate) fn ensure_joined_output(bound: Option<&str>) {
    imp::ensure_joined_output(bound);
}
