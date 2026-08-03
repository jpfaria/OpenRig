//! Typed `Command` enum — every state-change that any controller can request.
//!
//! One variant per current Slint `on_*` callback that mutates `session.project`.
//! Variants follow the spec's naming when the spec names them; new variants
//! use the same PascalCase, no-abbreviation convention.
//!
//! The variants live in per-domain sub-enums (`command::block`, `command::chain`,
//! …) and `Command` is `#[serde(untagged)]` over them, so the serialized form is
//! unchanged: `Command::Block(BlockCommand::AddBlock { .. })` is still
//! `{"AddBlock": { .. }}` on the wire, and the MCP/gRPC tool surface still has
//! one entry per leaf variant.
//!
//! **Spec reference:** `docs/superpowers/specs/2026-04-23-command-dispatch-architecture-design.md`
//! — "Shared Architecture / Types".
//!
//! **Audit reference:** `docs/superpowers/audits/2026-05-14-command-audit.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod block;
pub mod chain;
pub mod io_binding;
pub mod looper;
pub mod metronome;
pub mod midi;
pub mod plugin;
pub mod project;
pub mod selection;
pub mod settings;
pub mod tone_doctor;

pub use block::BlockCommand;
pub use chain::ChainCommand;
pub use io_binding::IoBindingCommand;
pub use looper::LooperCommand;
pub use metronome::MetronomeCommand;
pub use midi::MidiCommand;
pub use plugin::PluginCommand;
pub use project::ProjectCommand;
pub use selection::SelectionCommand;
pub use settings::SettingsCommand;
pub use tone_doctor::ToneDoctorCommand;

pub use crate::di_loader::DiLoopSource;
pub use ::project::chain::DiOutputRef;
pub use ::project::chain::EndpointRef;
pub use ::project::chain::LooperSpeed;
pub use domain::ids::{BlockId, ChainId};
pub use domain::io_binding::{ChannelMode, IoBinding};

/// Every state change the UI or any controller can request.
///
/// Fine-grained: one leaf variant per logical operation currently expressed as
/// a Slint `on_*` callback that mutates `ProjectSession.project`.
///
/// Leaf variants are grouped by domain concern into sub-enums, one module each;
/// `#[serde(untagged)]` keeps the wire format identical to a single flat enum —
/// the serialized form is the leaf variant name as-is (serde default).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Command {
    Block(BlockCommand),
    Chain(ChainCommand),
    Project(ProjectCommand),
    Selection(SelectionCommand),
    Midi(MidiCommand),
    Settings(SettingsCommand),
    IoBinding(IoBindingCommand),
    Plugin(PluginCommand),
    Metronome(MetronomeCommand),
    ToneDoctor(ToneDoctorCommand),
    /// #323: per-chain loopers — membership, transport, params, endpoints and
    /// the linked preset (phase 2).
    Looper(LooperCommand),
}

/// What [`SelectionCommand::ApplyRigNav`] does to the chain's rig input.
///
/// `Preset`/`Scene` carry the GUI sentinel `i32`: `>= 0` selects that
/// preset-position / scene number, `-1` adds, `-2` removes.
///
/// `StepPreset`/`StepScene` carry a relative delta (`+1` next, `-1`
/// previous) and wrap — a footswitch has no absolute position, it just
/// advances. The dispatcher resolves the delta against the live rig.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum RigNavKind {
    Preset(i32),
    Scene(i32),
    StepPreset(i32),
    StepScene(i32),
}

/// #323: what a looper transport tap does.
///
/// `Record` is the record/overdub footswitch: it starts the first recording,
/// closes it (freezing the loop length), starts an overdub, or closes the
/// overdub — whichever the looper's current state calls for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum LooperAction {
    Record,
    Play,
    Stop,
    /// Toggle: whoever applies it (the adapter, which can see the runtime)
    /// turns it into `Play` or `Stop`. A footswitch has one button for both.
    PlayStop,
    Undo,
    Redo,
    Clear,
}

/// #323: one persisted looper parameter.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum LooperParam {
    /// Loop level, 0..=1.
    Mix(f32),
    /// Per-layer-of-age gain on older layers, 0..=1 (1 = no decay).
    Decay(f32),
    Speed(LooperSpeed),
    Reverse(bool),
}
