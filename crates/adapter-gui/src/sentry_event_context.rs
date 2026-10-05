//! Responsibility: stamps the crash-report contexts onto every Sentry event (#1070).

use std::collections::BTreeMap;
use std::sync::Mutex;

use sentry::protocol::{Context, Event};
use serde_json::Value;

static AUDIO: Mutex<Option<Value>> = Mutex::new(None);

/// Replace the audio context future events carry. GUI thread only.
pub fn publish_audio(context: Value) {
    #[cfg(test)]
    LAST_ON_THREAD.with(|last| *last.borrow_mut() = Some(context.clone()));
    if let Ok(mut audio) = AUDIO.lock() {
        *audio = Some(context);
    }
}

/// `before_send` hook: never drops the event. Uses `try_lock` so an event
/// raised while a publish is in flight is sent without the audio context
/// instead of waiting.
pub fn attach(mut event: Event<'static>) -> Option<Event<'static>> {
    if let Some(audio) = AUDIO.try_lock().ok().and_then(|a| a.clone()) {
        event.contexts.insert("audio".into(), as_context(audio));
    }
    event.contexts.insert(
        "host".into(),
        as_context(crate::sentry_host_context::host_context()),
    );
    Some(event)
}

fn as_context(value: Value) -> Context {
    match value {
        Value::Object(map) => Context::Other(map.into_iter().collect::<BTreeMap<_, _>>()),
        other => Context::Other(BTreeMap::from([("value".to_string(), other)])),
    }
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
