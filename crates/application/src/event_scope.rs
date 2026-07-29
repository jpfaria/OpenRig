//! `Event::chain()` — which chain an event affected (#792 file split).
//!
//! Lives beside the enum rather than inside `event.rs`: the enum itself is the
//! transport contract and keeps growing with every feature, so the exhaustive
//! match that classifies its variants gets its own module (#791 pushed
//! `event.rs` past the file cap).

use crate::command::ChainId;
use crate::event::Event;

impl Event {
    /// The chain this event affected, if any. Project-wide events
    /// (`ProjectSaved`, `ProjectMutated`, …) return `None`. Used by the
    /// MIDI/MCP drain to re-sync exactly the chains a footswitch touched.
    pub fn chain(&self) -> Option<&ChainId> {
        match self {
            Event::ChainReloaded { chain }
            | Event::BlockParameterChanged { chain, .. }
            | Event::BlockEnabledChanged { chain, .. }
            | Event::BlockReplaced { chain, .. }
            | Event::BlockAdded { chain, .. }
            | Event::BlockRemoved { chain, .. }
            | Event::DeviceChanged { chain, .. }
            | Event::ChainAdded { chain }
            | Event::ChainRemoved { chain }
            | Event::ChainEnabledChanged { chain, .. }
            | Event::ChainMoved { chain, .. }
            | Event::ChainConfigured { chain }
            | Event::ChainSaved { chain }
            | Event::ChainInputEndpointsSaved { chain }
            | Event::ChainOutputEndpointsSaved { chain }
            | Event::ChainIoSaved { chain }
            | Event::InsertBlockSaved { chain, .. }
            | Event::ChainPresetLoaded { chain }
            | Event::ChainVolumeChanged { chain, .. }
            | Event::ChainIoBindingsChanged { chain, .. }
            | Event::BlockSelectionChanged { chain, .. }
            | Event::ChainDiLoopSourceChanged { chain }
            | Event::ChainDiLoopEnabledChanged { chain, .. }
            | Event::ChainDiLoopOutputChanged { chain }
            | Event::ChainToneDiagnosed { chain, .. }
            | Event::ChainToneFixApplied { chain, .. }
            // #323: per-chain looper events are all chain-scoped.
            | Event::ChainLooperAdded { chain, .. }
            | Event::ChainLooperRemoved { chain, .. }
            | Event::ChainLooperTransportChanged { chain, .. }
            | Event::ChainLooperParamChanged { chain, .. }
            | Event::ChainLooperAudioFileChanged { chain, .. }
            | Event::ChainLooperInputChanged { chain, .. }
            | Event::ChainLooperOutputChanged { chain, .. }
            | Event::ChainLooperPresetChanged { chain, .. } => Some(chain),
            Event::ProjectMutated
            | Event::AudioSettingsSaved
            | Event::ProjectLoaded
            | Event::ProjectSaved
            | Event::ProjectCreated
            // #436 F/G/H/E + sweep: app/project-wide events, no ChainId.
            | Event::LanguageChanged { .. }
            | Event::OutputMutedChanged { .. }
            | Event::RecentProjectRemoved { .. }
            | Event::RecentProjectRegistered { .. }
            | Event::RecentProjectInvalidated { .. }
            | Event::ChainPresetSaved { .. }
            | Event::ChainPresetDeleted { .. }
            | Event::TunerEnabledChanged { .. }
            | Event::SpectrumEnabledChanged { .. }
            | Event::MetronomeEnabledChanged { .. }
            | Event::MetronomeBpmChanged { .. }
            | Event::MetronomeTimeSignatureChanged { .. }
            | Event::MetronomeSubdivisionChanged { .. }
            | Event::MetronomeVolumeChanged { .. }
            | Event::MetronomeTimbreChanged { .. }
            | Event::MetronomeCountInChanged { .. }
            | Event::MetronomeOutputChanged { .. }
            | Event::MetronomeTapped
            | Event::CompactViewEnabledChanged { .. }
            | Event::MidiEnabledChanged { .. }
            | Event::McpEnabledChanged { .. }
            | Event::ProjectClosed
            // #513 / #493: MIDI device / mapping / learn events live at the
            // system or project root, not a single chain.
            | Event::MidiDevicesSaved
            | Event::MidiMappingSaved
            | Event::MidiLearnStarted
            | Event::MidiLearnStopped
            | Event::MidiEventReceived { .. }
            // #513: system-level paths event, never tied to a chain.
            | Event::PathsSaved
            // #561: catalog-wide reload, never tied to a single chain.
            | Event::PluginCatalogReloaded { .. }
            // #561 (expanded scope): per-plugin load/unload, also catalog-scope.
            | Event::PluginLoaded { .. }
            | Event::PluginUnloaded { .. }
            | Event::PluginLoadFailed { .. }
            // #576: offline render does not touch any chain in the live project.
            | Event::RenderCompleted { .. }
            | Event::Error { .. }
            // #716: I/O binding registry is a system-level concern, not tied to any chain.
            | Event::IoBindingRegistryChanged
            // #829: device enumeration is machine-wide, not chain-scoped.
            | Event::AudioDevicesRefreshed => None,
        }
    }
}
