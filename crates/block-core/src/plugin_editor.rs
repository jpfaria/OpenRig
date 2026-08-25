//! Responsibility: names what a hosted plugin exposes to the app beyond audio.

pub trait NamedModel {
    fn model_key(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
}

/// Opaque handle to an open plugin editor window.
///
/// Dropping the handle closes the window and releases all resources.
/// The concrete type is an implementation detail of the plugin host crate.
pub trait PluginEditorHandle: Send {
    /// Bring the already-open editor window back to the front.
    ///
    /// Called when the user re-opens an editor that is still held open, so the
    /// host reuses the existing plugin instance instead of creating a new one
    /// (some plugins break their module after a window close + reload).
    fn focus(&self) {}
}
