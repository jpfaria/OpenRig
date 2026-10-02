//! Responsibility: marks the allocations of the current thread as audio memory.
//!
//! Wiring the whole process kept every byte the app ever freed resident —
//! 1.2 GB with one chain. Only the audio's memory has to stay
//! resident, so the allocations of a thread marked here go to the audio zone
//! (`audio_zone_router`) and only that zone is wired. A scope marks a stretch
//! of work (building a chain runtime); an audio thread (the dsp-worker, a
//! device callback) is marked for life, so whatever a plugin allocates lazily
//! while it plays is audio memory too.
//!
//! The mark lives in a pthread key, never in a Rust `thread_local!`: the
//! router reads it from inside `malloc`, and a Rust thread-local may allocate
//! on its first access.

use std::marker::PhantomData;

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    extern "C" {
        fn pthread_key_create(
            key: *mut usize,
            destructor: Option<unsafe extern "C" fn(*mut c_void)>,
        ) -> i32;
        fn pthread_getspecific(key: usize) -> *mut c_void;
        fn pthread_setspecific(key: usize, value: *const c_void) -> i32;
    }

    static KEY: OnceLock<Option<usize>> = OnceLock::new();

    /// Creates the key once. The router calls it before it is installed, so
    /// `malloc` never runs this.
    pub(crate) fn ensure_key() -> bool {
        KEY.get_or_init(|| {
            let mut key = 0usize;
            (unsafe { pthread_key_create(&mut key, None) } == 0).then_some(key)
        })
        .is_some()
    }

    /// Whether the current thread is marked. Never allocates.
    pub(crate) fn is_marked() -> bool {
        match KEY.get() {
            Some(Some(key)) => !unsafe { pthread_getspecific(*key) }.is_null(),
            _ => false,
        }
    }

    pub(crate) fn set_marked(marked: bool) {
        if !ensure_key() {
            return;
        }
        if let Some(Some(key)) = KEY.get() {
            let value = if marked { 1usize } else { 0usize };
            unsafe { pthread_setspecific(*key, value as *const c_void) };
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::cell::Cell;

    thread_local! {
        static MARKED: Cell<bool> = const { Cell::new(false) };
    }

    pub(crate) fn is_marked() -> bool {
        MARKED.with(Cell::get)
    }

    pub(crate) fn set_marked(marked: bool) {
        MARKED.with(|cell| cell.set(marked));
    }
}

#[cfg(target_os = "macos")]
pub(crate) use imp::ensure_key;

/// While alive, the current thread's allocations are audio memory. Dropping
/// it restores the mark the thread had before, so scopes nest.
#[must_use = "the scope ends when this guard is dropped"]
pub struct AudioAllocScope {
    was_marked: bool,
    /// The mark belongs to the thread that took it.
    _not_send: PhantomData<*const ()>,
}

/// Marks the current thread's allocations as audio memory until the returned
/// guard drops.
pub fn audio_allocations() -> AudioAllocScope {
    crate::audio_zone_router::install();
    let was_marked = imp::is_marked();
    imp::set_marked(true);
    AudioAllocScope {
        was_marked,
        _not_send: PhantomData,
    }
}

impl Drop for AudioAllocScope {
    fn drop(&mut self) {
        imp::set_marked(self.was_marked);
    }
}

/// Marks the current thread as an audio thread for the rest of its life.
pub fn mark_audio_thread() {
    crate::audio_zone_router::install();
    imp::set_marked(true);
}

/// Whether the current thread's allocations count as audio memory now.
pub fn allocating_audio() -> bool {
    imp::is_marked()
}

#[cfg(test)]
#[path = "audio_alloc_scope_tests.rs"]
mod audio_alloc_scope_tests;
