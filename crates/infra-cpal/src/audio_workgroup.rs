//! Issue #670 — join the audio callback thread to the input device's OS
//! workgroup (macOS). The heavy chain DSP runs in the cpal input callback;
//! when the OS lets that thread drift off the audio I/O core cluster, its
//! cache — notably the ~290 KB of NAM A2 weights — is lost to the UI thread
//! between buffers and the next inference reloads cold, spiking a 64-frame
//! buffer past its deadline (the xrun / "crackle").
//!
//! Joining the device's `os_workgroup` tells the scheduler this thread is part
//! of that device's real-time work, so it is co-scheduled with the audio I/O
//! and kept cache-warm. Crucially, unlike a `THREAD_TIME_CONSTRAINT_POLICY`
//! promotion, a workgroup join does NOT reserve a real-time band, so it cannot
//! oversubscribe when several chains each run their own callback thread (that
//! oversubscription is what made the earlier RT promotion worse).
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

    thread_local! {
        static ATTEMPTED: Cell<bool> = const { Cell::new(false) };
    }

    pub(crate) fn ensure_joined() {
        ATTEMPTED.with(|attempted| {
            if attempted.get() {
                return;
            }
            attempted.set(true); // try at most once per thread, whatever happens

            let Some(device) = property_u32(SYSTEM_OBJECT, DEFAULT_INPUT_DEVICE) else {
                log::warn!("[#670] workgroup: no default input device (skipping join)");
                return;
            };
            let Some(workgroup) = device_workgroup(device) else {
                log::warn!(
                    "[#670] workgroup: input device {device} has no OS workgroup (skipping join)"
                );
                return;
            };
            // The thread never leaves the workgroup, so the join token must
            // outlive it — leak it (one tiny allocation per audio thread).
            let token = Box::leak(Box::new(JoinToken { _opaque: [0; 8] }));
            let rc = unsafe { os_workgroup_join(workgroup, token) };
            if rc == 0 {
                log::info!("[#670] audio callback thread joined the input device OS workgroup");
            } else {
                log::info!(
                    "[#670] workgroup: os_workgroup_join returned {rc} \
                     (EALREADY/already-a-member is harmless)"
                );
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub(crate) fn ensure_joined() {}
}

/// Join the current (audio callback) thread to the input device's OS workgroup
/// once. Cheap thread-local check after the first call. See module docs.
pub(crate) fn ensure_joined() {
    imp::ensure_joined();
}
