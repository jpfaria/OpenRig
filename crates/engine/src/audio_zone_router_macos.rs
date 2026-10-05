//! Responsibility: implements the audio zone router on top of macOS's malloc zones.
//!
//! The router is a `malloc_zone_t` registered first, so `malloc` and friends
//! reach it: new allocations go to the audio zone when the calling thread is
//! marked, to the system zone otherwise; `free`/`realloc`/`size` go to the
//! zone that owns the pointer. Nothing here may allocate, lock or panic — it
//! runs inside `malloc`. The router keeps no heap of its own, so its
//! introspection reports nothing and its locks are no-ops (out-of-process
//! tools such as `vmmap` skip it and list both real zones).

use std::ffi::{c_char, c_void};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::OnceLock;

use crate::audio_alloc_scope::{allocating_audio, ensure_key};

type Zone = MallocZone;

#[repr(C)]
pub(crate) struct MallocZone {
    reserved1: *mut c_void,
    reserved2: *mut c_void,
    size: Option<unsafe extern "C" fn(*mut Zone, *const c_void) -> usize>,
    malloc: Option<unsafe extern "C" fn(*mut Zone, usize) -> *mut c_void>,
    calloc: Option<unsafe extern "C" fn(*mut Zone, usize, usize) -> *mut c_void>,
    valloc: Option<unsafe extern "C" fn(*mut Zone, usize) -> *mut c_void>,
    free: Option<unsafe extern "C" fn(*mut Zone, *mut c_void)>,
    realloc: Option<unsafe extern "C" fn(*mut Zone, *mut c_void, usize) -> *mut c_void>,
    destroy: Option<unsafe extern "C" fn(*mut Zone)>,
    zone_name: *const c_char,
    batch_malloc: Option<unsafe extern "C" fn(*mut Zone, usize, *mut *mut c_void, u32) -> u32>,
    batch_free: Option<unsafe extern "C" fn(*mut Zone, *mut *mut c_void, u32)>,
    pub(crate) introspect: *const Introspection,
    version: u32,
    // version >= 5
    memalign: Option<unsafe extern "C" fn(*mut Zone, usize, usize) -> *mut c_void>,
    // version >= 6
    free_definite_size: Option<unsafe extern "C" fn(*mut Zone, *mut c_void, usize)>,
    // version >= 8
    pressure_relief: Option<unsafe extern "C" fn(*mut Zone, usize) -> usize>,
    // version >= 10
    claimed_address: Option<unsafe extern "C" fn(*mut Zone, *mut c_void) -> u32>,
}

#[repr(C)]
pub(crate) struct Introspection {
    pub(crate) enumerator: Option<
        unsafe extern "C" fn(u32, *mut c_void, u32, usize, *const c_void, *const c_void) -> i32,
    >,
    good_size: Option<unsafe extern "C" fn(*mut Zone, usize) -> usize>,
    check: Option<unsafe extern "C" fn(*mut Zone) -> u32>,
    print: Option<unsafe extern "C" fn(*mut Zone, u32)>,
    log: Option<unsafe extern "C" fn(*mut Zone, *mut c_void)>,
    pub(crate) force_lock: Option<unsafe extern "C" fn(*mut Zone)>,
    pub(crate) force_unlock: Option<unsafe extern "C" fn(*mut Zone)>,
    statistics: Option<unsafe extern "C" fn(*mut Zone, *mut Statistics)>,
    zone_locked: Option<unsafe extern "C" fn(*mut Zone) -> u32>,
    // version >= 7
    enable_discharge_checking: Option<unsafe extern "C" fn(*mut Zone) -> u32>,
    disable_discharge_checking: Option<unsafe extern "C" fn(*mut Zone)>,
    discharge: Option<unsafe extern "C" fn(*mut Zone, *mut c_void)>,
    enumerate_discharged_pointers: Option<unsafe extern "C" fn()>,
    // version >= 9
    reinit_lock: Option<unsafe extern "C" fn(*mut Zone)>,
}

#[repr(C)]
pub(crate) struct Statistics {
    blocks_in_use: u32,
    size_in_use: usize,
    max_size_in_use: usize,
    size_allocated: usize,
}

extern "C" {
    static mach_task_self_: u32;
    fn malloc_get_all_zones(
        task: u32,
        reader: *const c_void,
        addresses: *mut *mut usize,
        count: *mut u32,
    ) -> i32;
    fn malloc_create_zone(start_size: usize, flags: u32) -> *mut Zone;
    fn malloc_set_zone_name(zone: *mut Zone, name: *const c_char);
    fn malloc_zone_register(zone: *mut Zone);
    fn malloc_zone_unregister(zone: *mut Zone);
}

static SYSTEM: AtomicPtr<Zone> = AtomicPtr::new(null_mut());
static AUDIO: AtomicPtr<Zone> = AtomicPtr::new(null_mut());

/// `Sync` wrapper: the router's tables are written once, before registration.
struct Shared<T>(std::cell::UnsafeCell<T>);
unsafe impl<T> Sync for Shared<T> {}

static INTROSPECTION: Introspection = Introspection {
    enumerator: Some(no_enumeration),
    good_size: Some(good_size),
    check: Some(always_true),
    print: Some(print_nothing),
    log: Some(log_nothing),
    force_lock: Some(no_lock),
    force_unlock: Some(no_lock),
    statistics: Some(no_statistics),
    zone_locked: Some(never_true),
    enable_discharge_checking: Some(never_true),
    disable_discharge_checking: Some(no_lock),
    discharge: Some(log_nothing),
    enumerate_discharged_pointers: None,
    reinit_lock: Some(no_lock),
};

static ROUTER: Shared<Zone> = Shared(std::cell::UnsafeCell::new(MallocZone {
    reserved1: null_mut(),
    reserved2: null_mut(),
    size: Some(route_size),
    malloc: Some(route_malloc),
    calloc: Some(route_calloc),
    valloc: Some(route_valloc),
    free: Some(route_free),
    realloc: Some(route_realloc),
    destroy: Some(no_lock),
    zone_name: c"OpenRigRouter".as_ptr(),
    batch_malloc: Some(route_batch_malloc),
    batch_free: Some(route_batch_free),
    introspect: &INTROSPECTION,
    version: 10,
    memalign: Some(route_memalign),
    free_definite_size: Some(route_free_definite_size),
    pressure_relief: Some(no_relief),
    claimed_address: Some(route_claimed_address),
}));

fn system() -> *mut Zone {
    SYSTEM.load(Ordering::Acquire)
}

fn audio() -> *mut Zone {
    AUDIO.load(Ordering::Acquire)
}

/// The zone a new allocation of the calling thread belongs to.
fn target() -> *mut Zone {
    if allocating_audio() {
        audio()
    } else {
        system()
    }
}

unsafe fn zone_size(zone: *mut Zone, ptr: *const c_void) -> usize {
    match (*zone).size {
        Some(size) => size(zone, ptr),
        None => 0,
    }
}

/// The zone that owns `ptr`: the audio zone if it claims it, else the system.
unsafe fn owner(ptr: *const c_void) -> *mut Zone {
    if zone_size(audio(), ptr) != 0 {
        audio()
    } else {
        system()
    }
}

unsafe extern "C" fn route_size(_: *mut Zone, ptr: *const c_void) -> usize {
    match zone_size(system(), ptr) {
        0 => zone_size(audio(), ptr),
        size => size,
    }
}

unsafe extern "C" fn route_malloc(_: *mut Zone, size: usize) -> *mut c_void {
    let zone = target();
    (*zone).malloc.map_or(null_mut(), |f| f(zone, size))
}

unsafe extern "C" fn route_calloc(_: *mut Zone, count: usize, size: usize) -> *mut c_void {
    let zone = target();
    (*zone).calloc.map_or(null_mut(), |f| f(zone, count, size))
}

unsafe extern "C" fn route_valloc(_: *mut Zone, size: usize) -> *mut c_void {
    let zone = target();
    (*zone).valloc.map_or(null_mut(), |f| f(zone, size))
}

unsafe extern "C" fn route_memalign(_: *mut Zone, alignment: usize, size: usize) -> *mut c_void {
    let zone = target();
    match (*zone).memalign {
        Some(f) if (*zone).version >= 5 => f(zone, alignment, size),
        _ => null_mut(),
    }
}

unsafe extern "C" fn route_free(_: *mut Zone, ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    let zone = owner(ptr);
    if let Some(f) = (*zone).free {
        f(zone, ptr);
    }
}

unsafe extern "C" fn route_free_definite_size(_: *mut Zone, ptr: *mut c_void, size: usize) {
    let zone = owner(ptr);
    match (*zone).free_definite_size {
        Some(f) if (*zone).version >= 6 => f(zone, ptr, size),
        _ => route_free(zone, ptr),
    }
}

unsafe extern "C" fn route_realloc(_: *mut Zone, ptr: *mut c_void, size: usize) -> *mut c_void {
    if ptr.is_null() {
        return route_malloc(null_mut(), size);
    }
    let zone = owner(ptr);
    (*zone).realloc.map_or(null_mut(), |f| f(zone, ptr, size))
}

unsafe extern "C" fn route_batch_malloc(
    _: *mut Zone,
    size: usize,
    results: *mut *mut c_void,
    count: u32,
) -> u32 {
    let zone = target();
    (*zone)
        .batch_malloc
        .map_or(0, |f| f(zone, size, results, count))
}

unsafe extern "C" fn route_batch_free(_: *mut Zone, ptrs: *mut *mut c_void, count: u32) {
    for i in 0..count as usize {
        route_free(null_mut(), *ptrs.add(i));
    }
}

unsafe extern "C" fn route_claimed_address(_: *mut Zone, ptr: *mut c_void) -> u32 {
    (route_size(null_mut(), ptr) != 0) as u32
}

unsafe extern "C" fn good_size(_: *mut Zone, size: usize) -> usize {
    size
}

unsafe extern "C" fn no_enumeration(
    _: u32,
    _: *mut c_void,
    _: u32,
    _: usize,
    _: *const c_void,
    _: *const c_void,
) -> i32 {
    0
}

unsafe extern "C" fn always_true(_: *mut Zone) -> u32 {
    1
}

unsafe extern "C" fn never_true(_: *mut Zone) -> u32 {
    0
}

unsafe extern "C" fn print_nothing(_: *mut Zone, _: u32) {}

unsafe extern "C" fn log_nothing(_: *mut Zone, _: *mut c_void) {}

unsafe extern "C" fn no_lock(_: *mut Zone) {}

unsafe extern "C" fn no_statistics(_: *mut Zone, stats: *mut Statistics) {
    if !stats.is_null() {
        *stats = Statistics {
            blocks_in_use: 0,
            size_in_use: 0,
            max_size_in_use: 0,
            size_allocated: 0,
        };
    }
}

unsafe extern "C" fn no_relief(_: *mut Zone, _: usize) -> usize {
    0
}

/// The zones registered now, in malloc's lookup order.
fn registered_zones() -> Vec<*mut Zone> {
    let mut addresses: *mut usize = null_mut();
    let mut count = 0u32;
    let kr = unsafe {
        malloc_get_all_zones(
            mach_task_self_,
            std::ptr::null(),
            &mut addresses,
            &mut count,
        )
    };
    if kr != 0 || addresses.is_null() {
        return Vec::new();
    }
    (0..count as usize)
        .map(|i| unsafe { *addresses.add(i) } as *mut Zone)
        .collect()
}

fn install_now() -> bool {
    if !ensure_key() {
        return false;
    }
    let Some(&system_zone) = registered_zones().first() else {
        return false;
    };
    let audio_zone = unsafe { malloc_create_zone(0, 0) };
    if audio_zone.is_null() {
        return false;
    }
    unsafe { malloc_set_zone_name(audio_zone, c"OpenRigAudio".as_ptr()) };
    SYSTEM.store(system_zone, Ordering::Release);
    AUDIO.store(audio_zone, Ordering::Release);
    let router = ROUTER.0.get();
    unsafe {
        malloc_zone_register(router);
        // malloc serves from the first registered zone; unregistering the
        // system zone moves the last one (the router) into its slot.
        malloc_zone_unregister(system_zone);
        malloc_zone_register(system_zone);
    }
    let first = registered_zones().first().copied();
    if first != Some(router) {
        log::warn!(
            "audio memory: the router is not the first malloc zone; wiring stays process-wide"
        );
        return false;
    }
    true
}

static INSTALLED: OnceLock<bool> = OnceLock::new();

/// Installs the router once per process; later calls return the first
/// result. `false`: callers keep treating the whole process as audio.
pub fn install() -> bool {
    *INSTALLED.get_or_init(install_now)
}

/// The audio zone's address, once the router is installed.
pub(crate) fn audio_zone_address() -> Option<*mut Zone> {
    INSTALLED.get().copied().unwrap_or(false).then(audio)
}

/// Whether `ptr` was allocated in the audio zone.
pub fn is_audio_allocation(ptr: *const u8) -> bool {
    audio_zone_address().is_some_and(|zone| unsafe { zone_size(zone, ptr.cast()) } != 0)
}
