//! Responsibility: says which side of the audio settings the screen is showing.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioSettingsMode {
    Gui,
    Project,
}

// `ConfigYaml` moved into `Command::SaveProject`'s dispatcher
// handler (`application::local_dispatcher_project`) in #555 — the
// sidecar `config.yaml` is now written there with a fixed
// `presets_path: ./presets` body, matching what this struct produced.
