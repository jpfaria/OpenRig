//! #913 — the editor handle's default behaviour.
//!
//! `focus()` has a default body on purpose: a host whose editor cannot be
//! raised must still compile and must do NOTHING when the user re-opens an
//! editor that is already open. A default that panicked (or that hosts were
//! forced to implement) would crash the re-open path for every such host.

use super::{NamedModel, PluginEditorHandle};

struct Unraisable;
impl PluginEditorHandle for Unraisable {}

struct Raisable {
    raised: std::sync::atomic::AtomicUsize,
}
impl PluginEditorHandle for Raisable {
    fn focus(&self) {
        self.raised
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

struct Amp;
impl NamedModel for Amp {
    fn model_key(&self) -> &'static str {
        "american_clean"
    }
    fn display_name(&self) -> &'static str {
        "American Clean"
    }
}

#[test]
fn a_host_that_cannot_raise_its_window_still_answers_focus() {
    let handle: Box<dyn PluginEditorHandle> = Box::new(Unraisable);
    handle.focus();
    handle.focus();
}

#[test]
fn a_host_that_can_raise_its_window_is_asked_every_time() {
    let handle = Raisable {
        raised: std::sync::atomic::AtomicUsize::new(0),
    };
    handle.focus();
    handle.focus();
    assert_eq!(handle.raised.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[test]
fn a_named_model_reports_a_key_and_a_display_name_that_differ() {
    let amp = Amp;
    assert_eq!(amp.model_key(), "american_clean");
    assert_eq!(amp.display_name(), "American Clean");
    assert_ne!(
        amp.model_key(),
        amp.display_name(),
        "the key is an id, the display name is for the user"
    );
}
