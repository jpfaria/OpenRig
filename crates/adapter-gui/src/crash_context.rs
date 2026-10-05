//! Responsibility: holds the crash-report contexts every report carries.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde_json::Value;

static AUDIO: Mutex<Option<Value>> = Mutex::new(None);

/// Replace the audio context future reports carry. GUI thread only. When the
/// setup changed it is also written to the session log on disk, so the file
/// tells what was running even with no network; returns whether it changed.
pub fn publish_audio(context: Value) -> bool {
    #[cfg(test)]
    LAST_ON_THREAD.with(|last| *last.borrow_mut() = Some(context.clone()));
    {
        let Ok(mut audio) = AUDIO.lock() else {
            return false;
        };
        if audio.as_ref() == Some(&context) {
            return false;
        }
        *audio = Some(context.clone());
    }
    log::info!("audio context: {context}");
    true
}

/// The contexts for a report raised now: the published audio setup (when
/// any) and a fresh host sample. The lock is only ever held by a GUI-thread
/// compare-and-swap, never by the audio thread; a poisoned lock still yields
/// its value, and a panic while sampling the host only drops that context.
pub fn contexts() -> BTreeMap<String, Value> {
    let mut all = BTreeMap::new();
    let audio = AUDIO.lock().unwrap_or_else(|p| p.into_inner()).clone();
    if let Some(audio) = audio {
        all.insert("audio".to_string(), audio);
    }
    if let Ok(host) = std::panic::catch_unwind(crate::crash_context_host::host_context) {
        all.insert("host".to_string(), host);
    }
    all
}

#[cfg(test)]
thread_local! {
    static LAST_ON_THREAD: std::cell::RefCell<Option<Value>> = const { std::cell::RefCell::new(None) };
}

/// What this thread published last — unit tests run in parallel and share
/// `AUDIO`, so they read their own publish here.
#[cfg(test)]
pub(crate) fn last_published_on_this_thread() -> Option<Value> {
    LAST_ON_THREAD.with(|last| last.borrow().clone())
}
