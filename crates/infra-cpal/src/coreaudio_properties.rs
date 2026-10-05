//! Responsibility: reads CoreAudio object properties into Rust values.
//!
//! Plain `AudioObjectGetPropertyData` calls, for code off the audio thread.
//! A property the object does not have reads as `None`.

use std::os::raw::c_void;

#[repr(C)]
struct AudioObjectPropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyData(
        object: u32,
        address: *const AudioObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn AudioObjectGetPropertyDataSize(
        object: u32,
        address: *const AudioObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
    ) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringGetCString(
        the_string: *const c_void,
        buffer: *mut core::ffi::c_char,
        buffer_size: isize,
        encoding: u32,
    ) -> u8;
    fn CFStringGetLength(the_string: *const c_void) -> isize;
    fn CFRelease(cf: *const c_void);
}

/// `kAudioObjectSystemObject`.
pub(crate) const SYSTEM_OBJECT: u32 = 1;
pub(crate) const SCOPE_GLOBAL: u32 = fourcc(b"glob");
pub(crate) const SCOPE_INPUT: u32 = fourcc(b"inpt");
pub(crate) const SCOPE_OUTPUT: u32 = fourcc(b"outp");
/// `kAudioHardwarePropertyDevices`.
const HW_DEVICES: u32 = fourcc(b"dev#");
/// `kAudioDevicePropertyDeviceUID`.
pub(crate) const DEVICE_UID: u32 = fourcc(b"uid ");
const UTF8: u32 = 0x0800_0100;

pub(crate) const fn fourcc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

/// A FourCC as text (`lpcm`, `usb `).
pub(crate) fn fourcc_text(code: u32) -> String {
    code.to_be_bytes()
        .iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '?'
            }
        })
        .collect()
}

fn address(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        selector,
        scope,
        element: 0,
    }
}

/// The raw bytes of a property.
pub(crate) fn bytes(object: u32, selector: u32, scope: u32) -> Option<Vec<u8>> {
    let address = address(selector, scope);
    let mut size: u32 = 0;
    let status =
        unsafe { AudioObjectGetPropertyDataSize(object, &address, 0, std::ptr::null(), &mut size) };
    if status != 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            &address,
            0,
            std::ptr::null(),
            &mut size,
            data.as_mut_ptr() as *mut c_void,
        )
    };
    if status != 0 {
        return None;
    }
    data.truncate(size as usize);
    Some(data)
}

fn fixed<const N: usize>(object: u32, selector: u32, scope: u32) -> Option<[u8; N]> {
    bytes(object, selector, scope)?.get(..N)?.try_into().ok()
}

pub(crate) fn u32_of(object: u32, selector: u32, scope: u32) -> Option<u32> {
    fixed::<4>(object, selector, scope).map(u32::from_ne_bytes)
}

pub(crate) fn i32_of(object: u32, selector: u32, scope: u32) -> Option<i32> {
    fixed::<4>(object, selector, scope).map(i32::from_ne_bytes)
}

pub(crate) fn f32_of(object: u32, selector: u32, scope: u32) -> Option<f32> {
    fixed::<4>(object, selector, scope).map(f32::from_ne_bytes)
}

pub(crate) fn f64_of(object: u32, selector: u32, scope: u32) -> Option<f64> {
    fixed::<8>(object, selector, scope).map(f64::from_ne_bytes)
}

/// A property holding a list of `AudioObjectID`s.
pub(crate) fn objects_of(object: u32, selector: u32, scope: u32) -> Vec<u32> {
    bytes(object, selector, scope)
        .map(|data| {
            data.chunks_exact(4)
                .map(|id| u32::from_ne_bytes([id[0], id[1], id[2], id[3]]))
                .collect()
        })
        .unwrap_or_default()
}

/// A property holding a +1 retained `CFStringRef`; released here.
pub(crate) fn string_of(object: u32, selector: u32, scope: u32) -> Option<String> {
    let address = address(selector, scope);
    let mut cfstr: *const c_void = std::ptr::null();
    let mut size = std::mem::size_of::<*const c_void>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
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
    let len = unsafe { CFStringGetLength(cfstr) };
    let cap = (len.max(0) as usize) * 4 + 1;
    let mut buf = vec![0 as core::ffi::c_char; cap];
    let ok = unsafe { CFStringGetCString(cfstr, buf.as_mut_ptr(), cap as isize, UTF8) };
    unsafe { CFRelease(cfstr) };
    if ok == 0 {
        return None;
    }
    unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
        .to_str()
        .ok()
        .map(str::to_string)
}

/// Every audio device's `AudioObjectID`.
pub(crate) fn all_devices() -> Vec<u32> {
    objects_of(SYSTEM_OBJECT, HW_DEVICES, SCOPE_GLOBAL)
}

/// The device whose UID is the tail of a cpal `coreaudio:<uid>` id.
pub(crate) fn device_for_cpal_id(device_id: &str) -> Option<u32> {
    let uid = device_id.strip_prefix("coreaudio:").unwrap_or(device_id);
    all_devices()
        .into_iter()
        .find(|&d| string_of(d, DEVICE_UID, SCOPE_GLOBAL).as_deref() == Some(uid))
}
