//! Responsibility: keeps COM initialised for the whole process on Windows.

use std::sync::Once;

/// cpal creates its WASAPI device enumerator in the COM apartment of the
/// first thread that enumerates, and caches it for the process. When that
/// thread exits and no other thread holds COM, COM shuts down and can unload
/// the enumerator's DLL, so the next enumeration crashes with
/// STATUS_ACCESS_VIOLATION (#978). One MTA usage held for the life of the
/// process keeps COM up however many enumerating threads come and go.
pub(crate) fn keep_com_alive() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: a plain Win32 call with no pointers of ours. The cookie is
        // deliberately never passed to CoDecrementMTAUsage.
        if let Err(e) = unsafe { windows::Win32::System::Com::CoIncrementMTAUsage() } {
            log::warn!("CoIncrementMTAUsage failed, COM may shut down between device scans: {e}");
        }
    });
}
