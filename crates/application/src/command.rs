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
pub mod midi;
pub mod plugin;
pub mod project;
pub mod selection;
pub mod settings;

pub use block::BlockCommand;
pub use chain::ChainCommand;
pub use io_binding::IoBindingCommand;
pub use midi::MidiCommand;
pub use plugin::PluginCommand;
pub use project::ProjectCommand;
pub use selection::SelectionCommand;
pub use settings::SettingsCommand;

pub use crate::di_loader::DiLoopSource;
pub use ::project::chain::DiOutputRef;
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
