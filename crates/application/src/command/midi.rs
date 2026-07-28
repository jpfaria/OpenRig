//! MIDI commands: per-machine device selection, the project's binding map,
//! learn mode, event passthrough, and the adapter master switch.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Every state change scoped to the MIDI subsystem.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum MidiCommand {
    /// #513: persist the per-machine MIDI device selection (config.yaml).
    /// The dispatcher emits `MidiDevicesSaved` only; persistence happens
    /// in the adapter wiring, identical to `SaveAudioSettings`'s system-
    /// side counterpart (no project mutation here — MIDI devices are a
    /// system-level concept per ADR 0003).
    SaveMidiDevices {
        devices: Vec<infra_filesystem::MidiDeviceSelection>,
    },

    /// #513 / #493: replace the project's MIDI binding list. Writes
    /// `project.midi.bindings`. The adapter persists the project file
    /// after `Event::MidiMappingSaved` fans out.
    SaveMidiMapping {
        bindings: Vec<project::midi::Binding>,
    },

    /// #513 / #493: put the MIDI daemon into single-shot learn mode. The
    /// next received MIDI event is published as `MidiEventReceived` and
    /// the daemon returns to normal mode automatically.
    StartMidiLearn,

    /// #513 / #493: cancel an outstanding learn request (the user closed
    /// the editor or pressed Cancel before any event arrived).
    StopMidiLearn,

    /// #513 / #493: emitted by the MIDI daemon while learn-mode is active.
    /// The daemon submits this through the existing command bridge (#165
    /// / #22) instead of routing the event itself, so the event still
    /// reaches the GUI through `PublishingDispatcher`'s fan-out — one
    /// transport, one ordering invariant. The handler is a pure passthrough.
    PublishMidiEvent { source: project::midi::Source },

    /// #712: master switch for the MIDI/BLE-MIDI adapter, persisted into
    /// the per-machine `config.yaml` (`midi_enabled`). Set from the
    /// Settings toggle so packaged builds — launched with no CLI flags —
    /// can bring MIDI up. Per-machine (ADR 0003), distinct from the
    /// per-port `midi_devices[].enabled` selection. Takes effect on next
    /// launch (the adapter is wired at bootstrap).
    SetMidiEnabled { enabled: bool },
}
