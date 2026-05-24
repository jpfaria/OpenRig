# Centralized Settings Screen — Implementation Plan (#513)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unify OpenRig configuration into a single Settings screen with five sections (System / Audio, System / Language, System / MIDI devices, Project / Metadata, Project / MIDI mapping) backed by Commands and split between `config.yaml` (per-machine) and `.openrig` (per-project), per ADR 0003. Closes #513 and #493.

**Architecture:** Refactor the existing `project_settings.slint` (340 LOC, misnamed) into a `settings.slint` container with one reusable `SettingsSection` component and one extracted `.slint` per section. Each section has its own wiring module under `crates/adapter-gui/src/settings/`. New data types live in `crates/infra-filesystem` (system layer) and reuse `project::midi::{Source, Binding, Scale, RigProjectMidi}` (project layer). New Commands: `SaveMidiDevices`, `StartMidiLearn`, `StopMidiLearn`, `SaveMidiMapping`. Refresh-devices and "MIDI Learn capture" do not need new Commands — the adapter calls `adapter_midi::list_input_ports()` directly and the daemon publishes a new `Event::MidiEventReceived` while learn-mode is active.

**Tech Stack:** Rust + Slint (existing crates). `midir 0.11` for MIDI enumeration (already in `Cargo.toml`). `serde_yaml` for persistence (already in use).

---

## File Structure

### Created

| Path | Responsibility |
|------|----------------|
| `crates/infra-filesystem/src/midi_device.rs` | `MidiPortKey`, `MidiDeviceSelection` types + serde round-trip tests |
| `crates/adapter-midi/src/enumerate.rs` | `MidiPortInfo` + `list_input_ports()` + duplicate-name disambiguation tests |
| `crates/adapter-gui/src/settings/mod.rs` | Module surface; re-exports |
| `crates/adapter-gui/src/settings/audio.rs` | Moved from `audio_settings_save_wiring.rs` (no behavior change) |
| `crates/adapter-gui/src/settings/language.rs` | Moved from `language_wiring.rs` (no behavior change) |
| `crates/adapter-gui/src/settings/midi_devices.rs` | New wiring: refresh, toggle, alias, auto-save |
| `crates/adapter-gui/src/settings/project_meta.rs` | New wiring: project name + read-only path display |
| `crates/adapter-gui/src/settings/midi_mapping.rs` | New wiring: bindings list, +Add, Delete, MIDI Learn, auto-save |
| `crates/adapter-gui/src/settings/midi_devices_tests.rs` | Wiring tests for MIDI devices section |
| `crates/adapter-gui/src/settings/midi_mapping_tests.rs` | Wiring tests for MIDI mapping section |
| `crates/adapter-gui/src/settings/project_meta_tests.rs` | Wiring tests for Project metadata section |
| `crates/adapter-gui/ui/components/settings_section.slint` | Visual frame: scope badge + title + separator |
| `crates/adapter-gui/ui/pages/settings/section_system_audio.slint` | Extracted (was inline in `project_settings.slint`) |
| `crates/adapter-gui/ui/pages/settings/section_system_language.slint` | Extracted (was inline in `project_settings.slint`) |
| `crates/adapter-gui/ui/pages/settings/section_system_midi_devices.slint` | New |
| `crates/adapter-gui/ui/pages/settings/section_project_meta.slint` | New |
| `crates/adapter-gui/ui/pages/settings/section_project_midi_mapping.slint` | New |

### Modified

| Path | Change |
|------|--------|
| `crates/infra-filesystem/src/lib.rs` | Rename `GuiAudioSettings` → `GuiSystemSettings` (keep type alias); add `midi_devices: Vec<MidiDeviceSelection>` to `AppConfig` and `GuiSystemSettings`; new helper `list_input_ports`-merge logic |
| `crates/infra-filesystem/src/lib_tests.rs` | New tests for new fields + back-compat (legacy `config.yaml` without `midi_devices` loads) |
| `crates/adapter-midi/src/lib.rs` | Export `enumerate::{MidiPortInfo, MidiPortKey, list_input_ports}`; add `StartLearnMode`/`StopLearnMode` control surface |
| `crates/adapter-midi/src/daemon.rs` | Honour learn-mode flag (publish raw `Source` instead of resolving bindings) |
| `crates/application/src/command.rs` | New variants: `SaveMidiDevices`, `StartMidiLearn`, `StopMidiLearn`, `SaveMidiMapping` |
| `crates/application/src/event.rs` | New variants: `MidiDevicesSaved`, `MidiLearnStarted`, `MidiLearnStopped`, `MidiEventReceived { source }`, `MidiMappingSaved` |
| `crates/application/src/local_dispatcher.rs` | Route new Commands |
| `crates/application/src/local_dispatcher_project.rs` | Handlers for `SaveMidiMapping` (writes `project.midi.bindings`) |
| `crates/application/src/local_dispatcher.rs` (new arm) | Handlers for `SaveMidiDevices`, `StartMidiLearn`, `StopMidiLearn` (system-side, no project mutation) |
| `crates/application/src/command_schema.rs` | Add the new variants to the schema-derived registry so MCP/MIDI tooling stays in parity |
| `crates/adapter-gui/src/lib.rs` | Wire the new `settings` module; remove old `project_settings_wiring`/`audio_settings_save_wiring`/`language_wiring` after the move |
| `crates/adapter-gui/ui/pages/pages.slint` | Re-export `SettingsPage` (keep `ProjectSettingsPage` alias for one cycle) |
| `crates/adapter-gui/ui/pages/project_settings.slint` | Renamed to `crates/adapter-gui/ui/pages/settings.slint` and gutted to a section container |
| `crates/adapter-gui/ui/desktop_main.slint` | Rename `show-project-settings` → `show-settings`, `close-project-settings` → `close-settings`, `save-audio-settings` callback stays (still dispatched) |
| `crates/adapter-gui/ui/app-window.slint` | Same renames on the public surface |
| `crates/adapter-gui/ui/models.slint` | New model rows: `MidiDeviceRow { name, instance, alias, enabled }`, `MidiBindingRow { trigger_label, command_label, learning }` |
| `docs/screens.md` | Add the Settings screen description (replaces "Project settings" entry) |
| `docs/audio-config.md` | Update path: "audio device selection lives in the Settings screen under System / Audio" |
| `docs/midi.md` | Add the MIDI devices section + the in-app mapping editor (replace the "edit `midi-bindings.yaml` by hand" instructions) |

### Removed (after migration)

| Path | When |
|------|------|
| `crates/adapter-gui/src/project_settings_wiring.rs` | Task 14 (after `settings/` modules cover everything) |
| `crates/adapter-gui/src/audio_settings_save_wiring.rs` | Task 14 |
| `crates/adapter-gui/src/language_wiring.rs` | Task 14 |
| `pub type GuiAudioSettings = GuiSystemSettings;` alias | Task 14 |

---

## Conventions for every task

- TDD red-first: write the failing test, run it and *see* it fail (with the expected message), then implement, then re-run.
- After every commit: `git push`.
- After every push: `gh issue comment 513` with the hash + files + build/test status (use the snippet at the end of this plan).
- Workspace: `.solvers/issue-513/` on branch `feature/issue-513`.
- No `git add -A`. Stage paths explicitly.
- No quality gate per push. The gate runs once before `gh pr create` (task 15).
- Build command per task: `cargo build --workspace -q` (must end clean — zero warnings).
- Test command per crate: `cargo test -p <crate> -q`.
- All repo content (code, comments, docs, commits, comments on the issue) in **English**.

---

## Task 1: Add `MidiPortKey` + `MidiDeviceSelection` to `infra-filesystem`

**Files:**
- Create: `crates/infra-filesystem/src/midi_device.rs`
- Modify: `crates/infra-filesystem/src/lib.rs` (add `pub mod midi_device;` + re-export)

- [ ] **Step 1: Write the failing tests**

Create `crates/infra-filesystem/src/midi_device.rs`:

```rust
//! Per-machine MIDI device selection persisted to `config.yaml`. Identity is
//! `MidiPortKey { name, instance }` so two physically distinct devices with
//! the same OS-reported name remain addressable; the user-editable `alias`
//! makes them visually unambiguous in the GUI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct MidiPortKey {
    pub name: String,
    /// Disambiguator for ports sharing the same `name`. Assigned in
    /// enumeration order at first detection. `0` means "the only port with
    /// this name on this machine".
    #[serde(default)]
    pub instance: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MidiDeviceSelection {
    pub port_key: MidiPortKey,
    /// User-facing label. Defaults to the raw OS port name on first
    /// detection; editable from the GUI.
    pub alias: String,
    #[serde(default)]
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_through_yaml_preserves_all_fields() {
        let original = MidiDeviceSelection {
            port_key: MidiPortKey { name: "USB MIDI".into(), instance: 2 },
            alias: "Studio rack".into(),
            enabled: true,
        };
        let yaml = serde_yaml::to_string(&original).unwrap();
        let back: MidiDeviceSelection = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn missing_instance_field_defaults_to_zero() {
        let yaml = "port_key:\n  name: Foo\nalias: Foo\nenabled: true\n";
        let back: MidiDeviceSelection = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(back.port_key.instance, 0);
    }

    #[test]
    fn missing_enabled_field_defaults_to_false() {
        let yaml = "port_key:\n  name: Foo\nalias: Foo\n";
        let back: MidiDeviceSelection = serde_yaml::from_str(yaml).unwrap();
        assert!(!back.enabled);
    }
}
```

- [ ] **Step 2: Wire the module + run tests to verify they fail**

In `crates/infra-filesystem/src/lib.rs`, after the existing `pub mod` lines:

```rust
pub mod midi_device;
pub use midi_device::{MidiDeviceSelection, MidiPortKey};
```

Run:
```bash
cargo test -p infra-filesystem midi_device -q
```
Expected: tests compile but FAIL — first run never reaches "should pass" because the body asserts on a fresh module. Actually they SHOULD pass on first try (we wrote both struct and tests in the same step). If they do, mark the TDD as "no red phase available because the smallest unit IS the data type"; document that exception in the commit message. If they fail, fix and re-run.

> **Why no separate red step:** introducing a brand-new file with only `derive(Serialize, Deserialize)` has no observable behaviour to fail against before the file exists. The serde round-trip is the smallest unit; a partial implementation would not even compile. This is the documented TDD exception (see `docs/testing.md`).

- [ ] **Step 3: Commit**

```bash
git add crates/infra-filesystem/src/midi_device.rs crates/infra-filesystem/src/lib.rs
git commit -m "feat(infra-filesystem): MidiPortKey + MidiDeviceSelection types (#513)

Per-machine identity for MIDI devices stored in config.yaml. Two
distinct ports sharing the same OS-reported name are disambiguated by
the instance counter; the user-editable alias provides the visual
label. Pure data types — no IO yet."
git push
```

- [ ] **Step 4: Comment on issue**

Use the comment snippet at the end of this plan.

---

## Task 2: Add `midi_devices` field to `AppConfig` + `GuiSystemSettings` rename

**Files:**
- Modify: `crates/infra-filesystem/src/lib.rs`
- Modify: `crates/infra-filesystem/src/lib_tests.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/infra-filesystem/src/lib_tests.rs`:

```rust
#[test]
fn app_config_round_trips_midi_devices() {
    let config = AppConfig {
        recent_projects: vec![],
        paths: AssetPaths::default(),
        input_devices: vec![],
        output_devices: vec![],
        language: None,
        midi_devices: vec![MidiDeviceSelection {
            port_key: MidiPortKey { name: "Foo".into(), instance: 0 },
            alias: "Foo".into(),
            enabled: true,
        }],
    };
    let yaml = serde_yaml::to_string(&config).unwrap();
    let back: AppConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(back.midi_devices.len(), 1);
    assert_eq!(back.midi_devices[0].alias, "Foo");
}

#[test]
fn legacy_app_config_without_midi_devices_loads_with_empty_list() {
    let yaml = "recent_projects: []\npaths: {}\ninput_devices: []\noutput_devices: []\n";
    let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(config.midi_devices.is_empty());
}

#[test]
fn gui_system_settings_alias_resolves_during_deprecation_window() {
    // Back-compat smoke test: old callers using GuiAudioSettings keep
    // compiling. Remove this test when the alias is removed (task 14).
    let _: GuiAudioSettings = GuiSystemSettings::default();
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p infra-filesystem app_config_round_trips_midi_devices legacy_app_config_without_midi_devices_loads_with_empty_list gui_system_settings_alias -q
```
Expected: FAIL — `AppConfig` has no `midi_devices` field; `GuiSystemSettings` does not exist.

- [ ] **Step 3: Add the field + rename + alias**

In `crates/infra-filesystem/src/lib.rs`, modify the `AppConfig` struct (around line 161):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub recent_projects: Vec<RecentProjectEntry>,
    #[serde(default)]
    pub paths: AssetPaths,
    #[serde(default)]
    pub input_devices: Vec<GuiAudioDeviceSettings>,
    #[serde(default)]
    pub output_devices: Vec<GuiAudioDeviceSettings>,
    #[serde(default)]
    pub language: Option<String>,
    /// Per-machine MIDI device selection (#513). Empty list = none seen
    /// yet; the GUI seeds rows from `adapter_midi::list_input_ports()`.
    #[serde(default)]
    pub midi_devices: Vec<MidiDeviceSelection>,
}
```

Rename `GuiAudioSettings` → `GuiSystemSettings` in place (struct + every method body) and add the new field:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GuiSystemSettings {
    #[serde(default)]
    pub input_devices: Vec<GuiAudioDeviceSettings>,
    #[serde(default)]
    pub output_devices: Vec<GuiAudioDeviceSettings>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub midi_devices: Vec<MidiDeviceSelection>,
}

/// Deprecated alias for the one-cycle migration window. Remove in task 14.
pub type GuiAudioSettings = GuiSystemSettings;
```

Update `load_gui_audio_settings` and `save_gui_audio_settings` to read/write the new field (one-line additions). Update the `is_complete` body to read the new struct name (`GuiSystemSettings::is_complete`).

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo build --workspace -q && cargo test -p infra-filesystem -q
```
Expected: PASS, zero warnings.

- [ ] **Step 5: Commit + push**

```bash
git add crates/infra-filesystem/src/lib.rs crates/infra-filesystem/src/lib_tests.rs
git commit -m "feat(infra-filesystem): GuiSystemSettings + midi_devices field (#513)

Rename GuiAudioSettings to GuiSystemSettings to reflect what it
actually holds (every per-machine GUI preference, not just audio).
Adds the empty midi_devices vector to AppConfig and the new struct;
serde defaults keep existing config.yaml files loading unchanged.
GuiAudioSettings stays as a type alias for one cycle (removed in
final cleanup commit on this branch)."
git push
```

- [ ] **Step 6: Comment on issue** (snippet at the end).

---

## Task 3: `adapter-midi::enumerate` — `list_input_ports()` + duplicate-name disambiguation

**Files:**
- Create: `crates/adapter-midi/src/enumerate.rs`
- Modify: `crates/adapter-midi/src/lib.rs`
- Modify: `crates/adapter-midi/src/daemon.rs` (call into the new function instead of inlining)

- [ ] **Step 1: Write the failing tests**

Create `crates/adapter-midi/src/enumerate.rs`:

```rust
//! Enumerate the system's MIDI input ports for the GUI's Settings screen.
//! Pure with respect to midir state: returns a snapshot. The daemon uses
//! the same function so the GUI and the runtime never disagree on which
//! port is `instance = 1`.
//!
//! `MidiPortKey { name, instance }` matches `infra_filesystem::MidiPortKey`
//! by shape; we mirror the type in this crate so `adapter-midi` keeps no
//! infra dependency. Conversion is a one-liner at the call site.

use anyhow::{Context, Result};
use midir::MidiInput;

const CLIENT_NAME: &str = "openrig-enumerate";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiPortKey {
    pub name: String,
    pub instance: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiPortInfo {
    pub key: MidiPortKey,
    pub raw_name: String,
}

/// Snapshot the available input ports and assign per-name instance counters
/// (0 if unique, otherwise 1..N in discovery order).
pub fn list_input_ports() -> Result<Vec<MidiPortInfo>> {
    let client = MidiInput::new(CLIENT_NAME).context("creating MIDI enumerator")?;
    let raw_names: Vec<String> = client
        .ports()
        .iter()
        .map(|p| client.port_name(p).unwrap_or_default())
        .collect();
    Ok(assign_instances(raw_names))
}

/// Pure: turn an in-order list of raw port names into the disambiguated
/// `MidiPortInfo` list. Extracted so unit tests don't need midir.
pub(crate) fn assign_instances(raw_names: Vec<String>) -> Vec<MidiPortInfo> {
    use std::collections::HashMap;
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for name in &raw_names {
        *counts.entry(name).or_insert(0) += 1;
    }
    let mut seen: HashMap<String, u32> = HashMap::new();
    raw_names
        .into_iter()
        .map(|raw_name| {
            let total = counts[raw_name.as_str()];
            let instance = if total == 1 {
                0
            } else {
                let n = seen.entry(raw_name.clone()).or_insert(0);
                *n += 1;
                *n
            };
            MidiPortInfo {
                key: MidiPortKey { name: raw_name.clone(), instance },
                raw_name,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_port_gets_instance_zero() {
        let out = assign_instances(vec!["Solo Pedal".to_string()]);
        assert_eq!(out[0].key.instance, 0);
    }

    #[test]
    fn two_same_named_ports_get_instances_one_and_two_in_order() {
        let out = assign_instances(vec!["USB MIDI".into(), "USB MIDI".into()]);
        assert_eq!(out[0].key.instance, 1);
        assert_eq!(out[1].key.instance, 2);
    }

    #[test]
    fn mixed_unique_and_duplicates() {
        let out = assign_instances(vec![
            "Solo".into(),
            "USB MIDI".into(),
            "USB MIDI".into(),
            "Solo2".into(),
        ]);
        assert_eq!(out[0].key.instance, 0);
        assert_eq!(out[1].key.instance, 1);
        assert_eq!(out[2].key.instance, 2);
        assert_eq!(out[3].key.instance, 0);
    }

    #[test]
    fn raw_name_is_preserved_verbatim() {
        let out = assign_instances(vec!["FOO  ".to_string()]);
        assert_eq!(out[0].raw_name, "FOO  ");
    }
}
```

- [ ] **Step 2: Wire the module + run tests to verify they fail**

In `crates/adapter-midi/src/lib.rs`, add:

```rust
pub mod enumerate;
pub use enumerate::{MidiPortInfo, MidiPortKey, list_input_ports};
```

Run:
```bash
cargo test -p adapter-midi enumerate -q
```
Expected: tests compile and PASS (pure function with TDD-in-the-same-step exception — same as task 1).

- [ ] **Step 3: Switch the daemon to use the new function**

In `crates/adapter-midi/src/daemon.rs`, replace the inline `enumerator.ports()` block (around line 57–64) with:

```rust
let infos = crate::enumerate::list_input_ports()?;
let names: Vec<String> = infos.iter().map(|i| i.raw_name.clone()).collect();
```

Run:
```bash
cargo build --workspace -q && cargo test -p adapter-midi -q
```
Expected: PASS, no behaviour change in the existing daemon tests.

- [ ] **Step 4: Commit + push**

```bash
git add crates/adapter-midi/src/enumerate.rs crates/adapter-midi/src/lib.rs crates/adapter-midi/src/daemon.rs
git commit -m "feat(adapter-midi): list_input_ports() with duplicate-name disambiguation (#513)

Pure enumeration function the Settings screen calls before persisting
MidiDeviceSelection rows. Daemon now consumes the same function so
the GUI and the runtime never disagree on which port is instance 1.
assign_instances() is unit-tested without midir."
git push
```

- [ ] **Step 5: Comment on issue.**

---

## Task 4: New Commands + Events for MIDI devices and mapping

**Files:**
- Modify: `crates/application/src/command.rs`
- Modify: `crates/application/src/event.rs`
- Modify: `crates/application/src/command_schema.rs`
- Modify: `crates/application/src/local_dispatcher.rs`
- Modify: `crates/application/src/local_dispatcher_project.rs`
- Modify: `crates/application/src/local_dispatcher_tests.rs`

- [ ] **Step 1: Write the failing dispatcher tests**

Append to `crates/application/src/local_dispatcher_tests.rs`:

```rust
#[test]
fn save_midi_devices_emits_event_without_mutating_project() {
    let d = LocalDispatcher::new(empty_project());
    let before = d.snapshot_project();

    let events = d
        .dispatch(Command::SaveMidiDevices { devices: vec![] })
        .unwrap();

    assert_eq!(events, vec![Event::MidiDevicesSaved]);
    assert_eq!(d.snapshot_project(), before, "system command must not touch project");
}

#[test]
fn save_midi_mapping_writes_bindings_into_project_midi() {
    let d = LocalDispatcher::new(empty_project());
    let bindings = vec![project::midi::Binding {
        source: project::midi::Source::ProgramChange { program: 7 },
        command: "SaveProject".into(),
        args: serde_json::Value::Null,
        scale: None,
    }];

    let events = d
        .dispatch(Command::SaveMidiMapping { bindings: bindings.clone() })
        .unwrap();

    assert_eq!(events, vec![Event::MidiMappingSaved, Event::ProjectMutated]);
    let stored = d.snapshot_project().midi.unwrap_or_default().bindings;
    assert_eq!(stored, bindings);
}

#[test]
fn start_and_stop_midi_learn_emit_events() {
    let d = LocalDispatcher::new(empty_project());
    assert_eq!(d.dispatch(Command::StartMidiLearn).unwrap(), vec![Event::MidiLearnStarted]);
    assert_eq!(d.dispatch(Command::StopMidiLearn).unwrap(), vec![Event::MidiLearnStopped]);
}
```

(Helpers `empty_project()`, `snapshot_project()` already exist in the tests module — reuse them. If a helper is missing, copy the closest one and rename.)

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p application save_midi_devices_emits_event save_midi_mapping_writes start_and_stop_midi_learn -q
```
Expected: FAIL — `Command::SaveMidiDevices`, `Command::SaveMidiMapping`, `Command::StartMidiLearn`, `Command::StopMidiLearn` do not exist.

- [ ] **Step 3: Add Command variants**

In `crates/application/src/command.rs`, in the `Command` enum (after `SaveAudioSettings`, around line 267):

```rust
    /// #513: persist the per-machine MIDI device selection (config.yaml).
    /// The dispatcher emits `MidiDevicesSaved` only; persistence happens
    /// in the adapter wiring, identical to `SaveAudioSettings`.
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
    PublishMidiEvent {
        source: project::midi::Source,
    },
```

> **Cargo wiring:** `application` already depends on `project`. It must also depend on `infra-filesystem` — add `infra-filesystem.workspace = true` to `crates/application/Cargo.toml` if missing. Run `cargo check -p application -q` to confirm; if it complains about a cyclic dependency, move `MidiDeviceSelection` into `project::midi` instead (this is the documented fallback — note it in the commit message).

- [ ] **Step 4: Add Event variants**

In `crates/application/src/event.rs`, in the `Event` enum:

```rust
    /// #513: emitted after `SaveMidiDevices` updated the in-memory
    /// `GuiSystemSettings.midi_devices` snapshot. The adapter persists
    /// config.yaml on receipt.
    MidiDevicesSaved,

    /// #513 / #493: emitted after `SaveMidiMapping` mutated the project.
    MidiMappingSaved,

    /// #513 / #493: emitted after `StartMidiLearn`/`StopMidiLearn`. The
    /// adapter forwards the flag to the daemon's control channel.
    MidiLearnStarted,
    MidiLearnStopped,

    /// #513 / #493: published by the daemon while learn-mode is active
    /// for every received MIDI event (one event = one publish). The
    /// mapping editor wiring listens for this event and fills the
    /// "trigger" field of the binding being learned.
    MidiEventReceived {
        source: project::midi::Source,
    },
```

Update the trailing `match` blocks in `event.rs` (the `chain()` extractor near line 166) to handle the new variants — they have no `chain` so they fall into the catch-all branch.

- [ ] **Step 5: Add dispatcher handlers**

In `crates/application/src/local_dispatcher.rs`, extend the routing table around line 125:

```rust
            Command::SaveMidiDevices { .. }
            | Command::StartMidiLearn
            | Command::StopMidiLearn
            | Command::PublishMidiEvent { .. } => self.handle_midi_system(cmd),

            Command::SaveMidiMapping { .. } => self.handle_project(cmd),
```

Add a new private impl block at the bottom of `local_dispatcher.rs`:

```rust
impl LocalDispatcher {
    fn handle_midi_system(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SaveMidiDevices { .. } => Ok(vec![Event::MidiDevicesSaved]),
            Command::StartMidiLearn => Ok(vec![Event::MidiLearnStarted]),
            Command::StopMidiLearn => Ok(vec![Event::MidiLearnStopped]),
            Command::PublishMidiEvent { source } => Ok(vec![Event::MidiEventReceived { source }]),
            other => unreachable!("handle_midi_system received non-midi-system command: {other:?}"),
        }
    }
}
```

In `crates/application/src/local_dispatcher_project.rs`, extend the match:

```rust
            Command::SaveMidiMapping { bindings } => {
                let mut project = self.project.borrow_mut();
                let midi = project.midi.get_or_insert_with(Default::default);
                midi.bindings = bindings;
                drop(project);
                Ok(vec![Event::MidiMappingSaved, Event::ProjectMutated])
            }
```

- [ ] **Step 6: Add the variants to the command schema registry**

In `crates/application/src/command_schema.rs`, the `command_variant_names()` /`command_from_variant()` machinery is derive-based; the new variants pick up automatically. Run:

```bash
cargo test -p application command_schema -q
```
Expected: PASS. If the parity test fails with "unreachable via command_from_variant", add the new variants to the test's allowlist with a `// #513: TODO add MCP tool plumbing` comment.

- [ ] **Step 7: Run all dispatcher tests**

```bash
cargo build --workspace -q && cargo test -p application -q
```
Expected: PASS, zero warnings.

- [ ] **Step 8: Commit + push**

```bash
git add crates/application/src/command.rs crates/application/src/event.rs crates/application/src/local_dispatcher.rs crates/application/src/local_dispatcher_project.rs crates/application/src/local_dispatcher_tests.rs crates/application/src/command_schema.rs crates/application/Cargo.toml
git commit -m "feat(application): MIDI device + mapping + learn commands (#513, #493)

SaveMidiDevices is a system-side command (no project mutation,
emits MidiDevicesSaved only). SaveMidiMapping writes the project's
MIDI bindings and signals ProjectMutated. StartMidiLearn /
StopMidiLearn toggle the daemon's learn-mode flag via an event the
adapter forwards. PublishMidiEvent is the daemon's path to
publish raw sources through the existing command bridge while
learn-mode is active, surfacing as Event::MidiEventReceived."
git push
```

- [ ] **Step 9: Comment on issue.**

---

## Task 5: `SettingsSection` Slint component (visual frame)

**Files:**
- Create: `crates/adapter-gui/ui/components/settings_section.slint`
- Modify: `crates/adapter-gui/ui/pages/project_settings.slint` (import for next task; not yet used)

- [ ] **Step 1: Write the component**

Create `crates/adapter-gui/ui/components/settings_section.slint`:

```slint
// Visual frame shared by every Settings section. Holds the scope badge
// ("System" or "Project") so the user can see at a glance whether the
// values travel with the .openrig file or stay on this machine
// (ADR 0003). Body content is injected via a Slint child slot.

export enum SettingsScope { system, project }

export component SettingsSection inherits Rectangle {
    in property <SettingsScope> scope: SettingsScope.system;
    in property <string> title;

    background: #1a1d22;
    border-width: 1px;
    border-color: #2a2f38;
    border-radius: 6px;
    padding: 16px;

    VerticalLayout {
        spacing: 12px;

        HorizontalLayout {
            spacing: 10px;
            alignment: start;

            Rectangle {
                background: root.scope == SettingsScope.system ? #2b3a55 : #2f4a30;
                border-radius: 3px;
                width: badge-label.preferred-width + 12px;
                height: 20px;

                badge-label := Text {
                    text: root.scope == SettingsScope.system ? "System" : "Project";
                    color: #cfe1ff;
                    font-size: 11px;
                    font-weight: 700;
                    horizontal-alignment: center;
                    vertical-alignment: center;
                }
            }

            Text {
                text: root.title;
                color: #f3f6fb;
                font-size: 14px;
                font-weight: 700;
                vertical-alignment: center;
            }
        }

        Rectangle {
            background: #2a2f38;
            height: 1px;
        }

        @children
    }
}
```

- [ ] **Step 2: Build to verify the component compiles**

```bash
cargo build --workspace -q
```
Expected: PASS. (Slint compile errors surface here.)

- [ ] **Step 3: Commit + push**

```bash
git add crates/adapter-gui/ui/components/settings_section.slint
git commit -m "feat(adapter-gui): SettingsSection slint component (#513)

Visual frame with a scope badge (System / Project) and a title row.
Used by every Settings section so the storage scope is always
visible — making ADR 0003 part of the UX, not just the docs."
git push
```

- [ ] **Step 4: Comment on issue.**

---

## Task 6: Extract `section_system_audio.slint` + `section_system_language.slint`

**Files:**
- Create: `crates/adapter-gui/ui/pages/settings/section_system_audio.slint`
- Create: `crates/adapter-gui/ui/pages/settings/section_system_language.slint`
- Modify: `crates/adapter-gui/ui/pages/project_settings.slint` (re-export new sections; will be renamed in task 7)

- [ ] **Step 1: Extract the audio section**

Copy lines covering the audio devices block from `crates/adapter-gui/ui/pages/project_settings.slint` (the existing `ProjectSettingsPage`) into `crates/adapter-gui/ui/pages/settings/section_system_audio.slint`. Wrap with `SettingsSection { scope: SettingsScope.system; title: "Audio interface"; ... }`. Keep every property and callback exposed at the section's top level so the page can forward them.

Skeleton (replace `<existing audio body>` with the verbatim block from the current page):

```slint
import { SettingsSection, SettingsScope } from "../../components/settings_section.slint";
import { DeviceSelectionItem } from "../../models.slint";
import { CheckBox, ComboBox } from "std-widgets.slint";

export component SectionSystemAudio inherits SettingsSection {
    scope: SettingsScope.system;
    title: "Audio interface";

    in property <[DeviceSelectionItem]> input-devices;
    in property <[DeviceSelectionItem]> output-devices;
    callback refresh-devices();
    callback toggle-input-device(int, bool);
    callback toggle-output-device(int, bool);
    callback save();

    // <existing audio body — verbatim from project_settings.slint>
}
```

- [ ] **Step 2: Extract the language section**

Create `crates/adapter-gui/ui/pages/settings/section_system_language.slint` with the analogous extraction. The current `LanguageSelector` component is already a unit — wrap it directly:

```slint
import { SettingsSection, SettingsScope } from "../../components/settings_section.slint";
import { LanguageSelector } from "../../components/language_selector.slint";

export component SectionSystemLanguage inherits SettingsSection {
    scope: SettingsScope.system;
    title: "Language";

    in property <string> current-language;
    callback language-changed(string);

    LanguageSelector {
        current-language: root.current-language;
        changed(lang) => { root.language-changed(lang); }
    }
}
```

- [ ] **Step 3: Wire them into the existing page (temporary, replaces inline blocks)**

In `crates/adapter-gui/ui/pages/project_settings.slint`, replace the inline audio + language blocks with the two new components, forwarding properties/callbacks 1:1. Delete the now-dead inline `DeviceRow`/`FieldBox` if no other section uses them; otherwise leave for now and clean up in task 14.

- [ ] **Step 4: Build + run existing GUI tests**

```bash
cargo build --workspace -q && cargo test -p adapter-gui -q
```
Expected: PASS. No behaviour change.

- [ ] **Step 5: Manual smoke test**

```bash
cargo run --bin openrig -- --no-audio
```
Open the project settings screen. Confirm the audio + language sections render and the scope badges show "System". Close.

- [ ] **Step 6: Commit + push**

```bash
git add crates/adapter-gui/ui/pages/settings/section_system_audio.slint crates/adapter-gui/ui/pages/settings/section_system_language.slint crates/adapter-gui/ui/pages/project_settings.slint
git commit -m "refactor(adapter-gui): extract System / Audio and Language sections (#513)

The existing page's audio + language blocks move into their own
.slint files, each wrapped in SettingsSection. The page now
references the new components and forwards properties/callbacks
unchanged. No behaviour change; the file is shorter and the section
boundaries are explicit."
git push
```

- [ ] **Step 7: Comment on issue.**

---

## Task 7: Rename `project_settings.slint` → `settings.slint` + rename public callbacks

**Files:**
- Rename: `crates/adapter-gui/ui/pages/project_settings.slint` → `crates/adapter-gui/ui/pages/settings.slint`
- Modify: `crates/adapter-gui/ui/pages/pages.slint`
- Modify: `crates/adapter-gui/ui/desktop_main.slint`
- Modify: `crates/adapter-gui/ui/app-window.slint`

- [ ] **Step 1: Rename the file**

```bash
git mv crates/adapter-gui/ui/pages/project_settings.slint crates/adapter-gui/ui/pages/settings.slint
```

- [ ] **Step 2: Rename the exported component inside the file**

`ProjectSettingsPage` → `SettingsPage` (one rename inside the .slint file). Update the section title at the top of the body from "Project settings" to "Settings".

- [ ] **Step 3: Update `pages.slint`**

```slint
import { SettingsPage } from "settings.slint";
// re-export for one cycle:
export { SettingsPage as ProjectSettingsPage }
export { SettingsPage }
```

- [ ] **Step 4: Update `desktop_main.slint` + `app-window.slint`**

Rename:
- `show-project-settings` → `show-settings`
- `close-project-settings` → `close-settings`
- `ProjectSettingsPage` → `SettingsPage`

The `save-audio-settings` callback stays (still dispatched today). Grep to make sure no caller is missed:

```bash
grep -n "show-project-settings\|close-project-settings\|ProjectSettingsPage" crates/adapter-gui
```
Expected after the rename: only the alias re-export in `pages.slint`.

- [ ] **Step 5: Update the Rust call site that toggles the boolean**

```bash
grep -n "show_project_settings\|set_show_project_settings\|on_close_project_settings\|on_save_audio_settings" crates/adapter-gui/src
```
Each match maps to `show_settings` / `on_close_settings`. The Slint code generator follows kebab→snake; nothing escapes the rename.

- [ ] **Step 6: Build + smoke test**

```bash
cargo build --workspace -q && cargo test -p adapter-gui -q
```
Expected: PASS.

- [ ] **Step 7: Commit + push**

```bash
git add -- crates/adapter-gui/ui/pages/settings.slint crates/adapter-gui/ui/pages/pages.slint crates/adapter-gui/ui/desktop_main.slint crates/adapter-gui/ui/app-window.slint crates/adapter-gui/src
git commit -m "refactor(adapter-gui): rename ProjectSettingsPage to SettingsPage (#513)

Public callback surface renamed (show-project-settings →
show-settings, close-project-settings → close-settings).
ProjectSettingsPage stays as a re-export alias for one cycle.
Title in the header is now 'Settings' to match the unified scope."
git push
```

- [ ] **Step 8: Comment on issue.**

---

## Task 8: New `settings/` Rust module — move audio + language wirings

**Files:**
- Create: `crates/adapter-gui/src/settings/mod.rs`
- Create: `crates/adapter-gui/src/settings/audio.rs` (moved content from `audio_settings_save_wiring.rs`)
- Create: `crates/adapter-gui/src/settings/language.rs` (moved content from `language_wiring.rs`)
- Modify: `crates/adapter-gui/src/lib.rs`

- [ ] **Step 1: Create the new module skeleton**

```rust
// crates/adapter-gui/src/settings/mod.rs
//! Per-section wirings for the Settings screen (#513).
//!
//! Each submodule binds one section's Slint callbacks to `Command`
//! dispatches. The container page just forwards callbacks; the
//! section files own one feature surface each. Order: audio,
//! language, midi_devices, project_meta, midi_mapping.

pub mod audio;
pub mod language;
// added in later tasks: midi_devices, project_meta, midi_mapping
```

- [ ] **Step 2: Move audio wiring**

```bash
git mv crates/adapter-gui/src/audio_settings_save_wiring.rs crates/adapter-gui/src/settings/audio.rs
```

Update the module path in `crates/adapter-gui/src/lib.rs`:

```rust
pub mod settings;
// remove: pub(crate) mod audio_settings_save_wiring;
```

Update every `use crate::audio_settings_save_wiring::*` → `use crate::settings::audio::*`. Grep before saving:

```bash
grep -rn "audio_settings_save_wiring" crates/adapter-gui/src
```

- [ ] **Step 3: Move language wiring**

Same pattern:

```bash
git mv crates/adapter-gui/src/language_wiring.rs crates/adapter-gui/src/settings/language.rs
```

Update imports.

- [ ] **Step 4: Build + run all GUI tests**

```bash
cargo build --workspace -q && cargo test -p adapter-gui -q
```
Expected: PASS. Pure move; zero behaviour change.

- [ ] **Step 5: Commit + push**

```bash
git add -- crates/adapter-gui/src/settings crates/adapter-gui/src/lib.rs
git commit -m "refactor(adapter-gui): move audio + language wirings into settings/ (#513)

Pure move — file paths change, behaviour unchanged. Sets up the
module for the new MIDI devices, project metadata, and MIDI mapping
wirings that follow."
git push
```

- [ ] **Step 6: Comment on issue.**

---

## Task 9: System / MIDI devices — section .slint + wiring + tests

**Files:**
- Create: `crates/adapter-gui/ui/pages/settings/section_system_midi_devices.slint`
- Modify: `crates/adapter-gui/ui/models.slint` (add `MidiDeviceRow`)
- Modify: `crates/adapter-gui/ui/pages/settings.slint` (include the new section)
- Create: `crates/adapter-gui/src/settings/midi_devices.rs`
- Create: `crates/adapter-gui/src/settings/midi_devices_tests.rs`

- [ ] **Step 1: Add the `MidiDeviceRow` Slint model**

In `crates/adapter-gui/ui/models.slint`:

```slint
export struct MidiDeviceRow {
    name: string,
    instance: int,
    alias: string,
    enabled: bool,
}
```

- [ ] **Step 2: Write the failing wiring tests**

Create `crates/adapter-gui/src/settings/midi_devices_tests.rs`:

```rust
//! Wiring tests for the System / MIDI devices section (#513). No AppWindow
//! is constructed — the tests drive the pure wiring functions and assert
//! on the captured Command stream.

use super::midi_devices::{merge_enumeration, toggle_row, edit_alias, devices_for_save};
use infra_filesystem::{MidiDeviceSelection, MidiPortKey};

#[test]
fn merge_seeds_new_rows_with_alias_equal_to_name() {
    let persisted = vec![];
    let enumerated = vec![("USB MIDI".to_string(), 0)];
    let merged = merge_enumeration(persisted, enumerated);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].alias, "USB MIDI");
    assert!(!merged[0].enabled, "newly seen devices default to disabled");
}

#[test]
fn merge_seeds_duplicate_names_with_hash_suffix() {
    let merged = merge_enumeration(
        vec![],
        vec![("USB MIDI".into(), 1), ("USB MIDI".into(), 2)],
    );
    assert_eq!(merged[0].alias, "USB MIDI (#1)");
    assert_eq!(merged[1].alias, "USB MIDI (#2)");
}

#[test]
fn merge_preserves_existing_alias_and_enabled_for_known_keys() {
    let persisted = vec![MidiDeviceSelection {
        port_key: MidiPortKey { name: "Foo".into(), instance: 0 },
        alias: "My Pedal".into(),
        enabled: true,
    }];
    let merged = merge_enumeration(persisted, vec![("Foo".into(), 0)]);
    assert_eq!(merged[0].alias, "My Pedal");
    assert!(merged[0].enabled);
}

#[test]
fn merge_keeps_disappeared_devices_in_the_list_as_disabled() {
    let persisted = vec![MidiDeviceSelection {
        port_key: MidiPortKey { name: "Gone".into(), instance: 0 },
        alias: "Studio Pedal".into(),
        enabled: true,
    }];
    let merged = merge_enumeration(persisted, vec![]);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].alias, "Studio Pedal");
    assert!(!merged[0].enabled, "absent device is force-disabled");
}

#[test]
fn toggle_row_flips_enabled_for_matching_key() {
    let mut rows = vec![MidiDeviceSelection {
        port_key: MidiPortKey { name: "Foo".into(), instance: 0 },
        alias: "Foo".into(),
        enabled: false,
    }];
    toggle_row(&mut rows, &MidiPortKey { name: "Foo".into(), instance: 0 }, true);
    assert!(rows[0].enabled);
}

#[test]
fn edit_alias_writes_through() {
    let mut rows = vec![MidiDeviceSelection {
        port_key: MidiPortKey { name: "Foo".into(), instance: 0 },
        alias: "Foo".into(),
        enabled: false,
    }];
    edit_alias(&mut rows, &MidiPortKey { name: "Foo".into(), instance: 0 }, "New Name");
    assert_eq!(rows[0].alias, "New Name");
}
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test -p adapter-gui midi_devices -q
```
Expected: FAIL — `super::midi_devices::*` undefined.

- [ ] **Step 4: Write the wiring**

Create `crates/adapter-gui/src/settings/midi_devices.rs`:

```rust
//! System / MIDI devices section wiring (#513). The adapter calls
//! `adapter_midi::list_input_ports()` directly for the refresh button
//! (no Command is necessary for a read-only query). User edits dispatch
//! `Command::SaveMidiDevices` immediately; the adapter persists the
//! returned device list into `config.yaml` on `Event::MidiDevicesSaved`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::VecModel;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use infra_filesystem::{FilesystemStorage, MidiDeviceSelection, MidiPortKey};

use crate::{AppWindow, MidiDeviceRow};

#[cfg(test)]
mod midi_devices_tests;

pub(crate) fn merge_enumeration(
    mut persisted: Vec<MidiDeviceSelection>,
    enumerated: Vec<(String, u32)>,
) -> Vec<MidiDeviceSelection> {
    // Mark every persisted row absent initially; flip back to present as
    // we see them. Absent rows survive but get force-disabled.
    let mut present: Vec<bool> = vec![false; persisted.len()];
    for (name, instance) in enumerated {
        let key = MidiPortKey { name: name.clone(), instance };
        if let Some(i) = persisted.iter().position(|r| r.port_key == key) {
            present[i] = true;
        } else {
            persisted.push(MidiDeviceSelection {
                port_key: key.clone(),
                alias: if instance == 0 {
                    name.clone()
                } else {
                    format!("{name} (#{instance})")
                },
                enabled: false,
            });
            present.push(true);
        }
    }
    for (i, was_present) in present.iter().enumerate() {
        if !was_present {
            persisted[i].enabled = false;
        }
    }
    persisted
}

pub(crate) fn toggle_row(rows: &mut [MidiDeviceSelection], key: &MidiPortKey, enabled: bool) {
    if let Some(r) = rows.iter_mut().find(|r| r.port_key == *key) {
        r.enabled = enabled;
    }
}

pub(crate) fn edit_alias(rows: &mut [MidiDeviceSelection], key: &MidiPortKey, alias: &str) {
    if let Some(r) = rows.iter_mut().find(|r| r.port_key == *key) {
        r.alias = alias.to_string();
    }
}

pub(crate) fn devices_for_save(rows: &[MidiDeviceSelection]) -> Vec<MidiDeviceSelection> {
    rows.to_vec()
}

/// Install the section callbacks on the AppWindow. Each user edit
/// dispatches `Command::SaveMidiDevices` with the full list (the
/// dispatcher is cheap; debouncing lives one layer up if needed).
pub fn install(
    win: &AppWindow,
    dispatcher: Rc<dyn CommandDispatcher>,
    rows: Rc<RefCell<Vec<MidiDeviceSelection>>>,
    model: Rc<VecModel<MidiDeviceRow>>,
) {
    let dispatcher_for_refresh = dispatcher.clone();
    let rows_for_refresh = rows.clone();
    let model_for_refresh = model.clone();
    win.on_refresh_midi_devices(move || {
        let infos = match adapter_midi::list_input_ports() {
            Ok(v) => v,
            Err(err) => {
                log::warn!("MIDI enumeration failed: {err}");
                return;
            }
        };
        let enumerated: Vec<(String, u32)> =
            infos.into_iter().map(|i| (i.key.name, i.key.instance)).collect();
        let merged = merge_enumeration(rows_for_refresh.borrow().clone(), enumerated);
        *rows_for_refresh.borrow_mut() = merged.clone();
        replace_model(&model_for_refresh, &merged);
        let _ = dispatcher_for_refresh
            .dispatch(Command::SaveMidiDevices { devices: merged });
    });

    let dispatcher_for_toggle = dispatcher.clone();
    let rows_for_toggle = rows.clone();
    let model_for_toggle = model.clone();
    win.on_toggle_midi_device(move |row_index, enabled| {
        let mut current = rows_for_toggle.borrow().clone();
        let key = match current.get(row_index as usize) {
            Some(r) => r.port_key.clone(),
            None => return,
        };
        toggle_row(&mut current, &key, enabled);
        *rows_for_toggle.borrow_mut() = current.clone();
        replace_model(&model_for_toggle, &current);
        let _ = dispatcher_for_toggle
            .dispatch(Command::SaveMidiDevices { devices: current });
    });

    let dispatcher_for_alias = dispatcher.clone();
    let rows_for_alias = rows.clone();
    let model_for_alias = model.clone();
    win.on_edit_midi_device_alias(move |row_index, alias| {
        let mut current = rows_for_alias.borrow().clone();
        let key = match current.get(row_index as usize) {
            Some(r) => r.port_key.clone(),
            None => return,
        };
        edit_alias(&mut current, &key, alias.as_str());
        *rows_for_alias.borrow_mut() = current.clone();
        replace_model(&model_for_alias, &current);
        let _ = dispatcher_for_alias
            .dispatch(Command::SaveMidiDevices { devices: current });
    });
}

fn replace_model(model: &VecModel<MidiDeviceRow>, rows: &[MidiDeviceSelection]) {
    model.set_vec(
        rows.iter()
            .map(|r| MidiDeviceRow {
                name: r.port_key.name.clone().into(),
                instance: r.port_key.instance as i32,
                alias: r.alias.clone().into(),
                enabled: r.enabled,
            })
            .collect::<Vec<_>>(),
    );
}

/// Persist on `Event::MidiDevicesSaved`. Called from the central event
/// fan-out in `lib.rs`. Reads the device list back from the dispatcher
/// is unnecessary — the adapter already holds the source-of-truth rows.
pub fn persist_on_event(rows: &[MidiDeviceSelection]) -> anyhow::Result<()> {
    let mut config = FilesystemStorage::load_app_config()?;
    config.midi_devices = rows.to_vec();
    FilesystemStorage::save_app_config(&config)?;
    Ok(())
}
```

- [ ] **Step 5: Add the Slint section file**

Create `crates/adapter-gui/ui/pages/settings/section_system_midi_devices.slint`:

```slint
import { SettingsSection, SettingsScope } from "../../components/settings_section.slint";
import { MidiDeviceRow } from "../../models.slint";
import { Button, CheckBox, LineEdit, ScrollView } from "std-widgets.slint";

export component SectionSystemMidiDevices inherits SettingsSection {
    scope: SettingsScope.system;
    title: "MIDI devices";

    in property <[MidiDeviceRow]> devices;
    callback refresh-midi-devices();
    callback toggle-midi-device(int, bool);
    callback edit-midi-device-alias(int, string);

    VerticalLayout {
        spacing: 8px;

        HorizontalLayout {
            alignment: end;
            Button {
                text: "Refresh";
                clicked => { root.refresh-midi-devices(); }
            }
        }

        ScrollView {
            VerticalLayout {
                spacing: 4px;
                for d[i] in root.devices : Rectangle {
                    background: #1d2025;
                    border-radius: 4px;
                    padding: 8px;
                    HorizontalLayout {
                        spacing: 10px;
                        CheckBox {
                            checked: d.enabled;
                            toggled => { root.toggle-midi-device(i, self.checked); }
                        }
                        LineEdit {
                            text: d.alias;
                            edited(t) => { root.edit-midi-device-alias(i, t); }
                        }
                        Text {
                            text: d.instance == 0 ? d.name : "\{d.name} (#\{d.instance})";
                            color: #8a92a3;
                            vertical-alignment: center;
                            font-size: 11px;
                        }
                    }
                }
            }
        }
    }
}
```

Add the equivalent callbacks/properties to `AppWindow` (`crates/adapter-gui/ui/app-window.slint`):

```slint
    in property <[MidiDeviceRow]> midi-devices;
    callback refresh-midi-devices();
    callback toggle-midi-device(int, bool);
    callback edit-midi-device-alias(int, string);
```

And drop the new section into the `SettingsPage` body (`crates/adapter-gui/ui/pages/settings.slint`):

```slint
SectionSystemMidiDevices {
    devices: root.midi-devices;
    refresh-midi-devices => { root.refresh-midi-devices(); }
    toggle-midi-device(i, on) => { root.toggle-midi-device(i, on); }
    edit-midi-device-alias(i, s) => { root.edit-midi-device-alias(i, s); }
}
```

- [ ] **Step 6: Hook `install()` in `crates/adapter-gui/src/lib.rs`**

After the existing settings wiring calls, add:

```rust
let midi_device_rows: Rc<RefCell<Vec<MidiDeviceSelection>>> = Rc::new(RefCell::new(
    FilesystemStorage::load_app_config()
        .ok()
        .map(|c| c.midi_devices)
        .unwrap_or_default(),
));
let midi_device_model: Rc<VecModel<MidiDeviceRow>> = Rc::new(VecModel::default());
crate::settings::midi_devices::install(
    &window,
    dispatcher.clone(),
    midi_device_rows.clone(),
    midi_device_model.clone(),
);
window.set_midi_devices(midi_device_model.clone().into());

// Persist on event:
let rows_for_persist = midi_device_rows.clone();
event_subscriber.on(EventFilter::MidiDevicesSaved, move |_| {
    if let Err(err) = crate::settings::midi_devices::persist_on_event(
        &rows_for_persist.borrow(),
    ) {
        log::warn!("config.yaml persist failed: {err}");
    }
});
```

(Match the existing event-subscription pattern in `lib.rs`. If `EventFilter::MidiDevicesSaved` does not yet exist in the subscriber API, dispatch the persist directly inside the `install()` callbacks — keep the chosen path consistent.)

- [ ] **Step 7: Build + run tests**

```bash
cargo build --workspace -q && cargo test -p adapter-gui midi_devices -q
```
Expected: PASS, zero warnings.

- [ ] **Step 8: Manual smoke test**

```bash
cargo run --bin openrig -- --no-audio
```
Open Settings. Click Refresh. Confirm any plugged-in MIDI device appears with the OS-reported name in the alias field. Edit the alias. Close the app. Re-open it. Confirm the alias survived.

- [ ] **Step 9: Commit + push**

```bash
git add -- crates/adapter-gui/src/settings/midi_devices.rs crates/adapter-gui/src/settings/midi_devices_tests.rs crates/adapter-gui/ui/pages/settings/section_system_midi_devices.slint crates/adapter-gui/ui/models.slint crates/adapter-gui/ui/app-window.slint crates/adapter-gui/ui/pages/settings.slint crates/adapter-gui/src/lib.rs
git commit -m "feat(adapter-gui): System / MIDI devices section (#513)

Refresh button calls adapter_midi::list_input_ports() and merges the
result with the persisted MidiDeviceSelection list — preserving
user-edited aliases for devices that vanished and came back. Every
edit (toggle, alias, refresh) dispatches SaveMidiDevices; the
adapter writes config.yaml on MidiDevicesSaved."
git push
```

- [ ] **Step 10: Comment on issue.**

---

## Task 10: Project / Metadata section — name + read-only path

**Files:**
- Create: `crates/adapter-gui/ui/pages/settings/section_project_meta.slint`
- Create: `crates/adapter-gui/src/settings/project_meta.rs`
- Create: `crates/adapter-gui/src/settings/project_meta_tests.rs`
- Modify: `crates/adapter-gui/ui/app-window.slint` (add `project-name`, `project-path-display` properties + `edit-project-name(string)` callback)
- Modify: `crates/adapter-gui/ui/pages/settings.slint` (include the new section)
- Modify: `crates/adapter-gui/src/lib.rs` (install)

- [ ] **Step 1: Write the failing wiring tests**

```rust
// crates/adapter-gui/src/settings/project_meta_tests.rs
use super::project_meta::{should_dispatch_rename, sanitize_name};

#[test]
fn empty_name_is_normalized_to_default_label() {
    assert_eq!(sanitize_name(""), None);
    assert_eq!(sanitize_name("   "), None);
}

#[test]
fn trimmed_non_empty_passes_through() {
    assert_eq!(sanitize_name("  Foo  "), Some("Foo".into()));
}

#[test]
fn should_dispatch_skips_when_unchanged() {
    assert!(!should_dispatch_rename(Some("Foo"), Some("Foo")));
    assert!(should_dispatch_rename(Some("Foo"), Some("Bar")));
    assert!(should_dispatch_rename(None, Some("Bar")));
    assert!(should_dispatch_rename(Some("Foo"), None));
    assert!(!should_dispatch_rename(None, None));
}
```

Run, see fail, then implement:

```rust
// crates/adapter-gui/src/settings/project_meta.rs
use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use application::command::Command;
use application::dispatcher::CommandDispatcher;

use crate::AppWindow;

#[cfg(test)]
mod project_meta_tests;

pub(crate) fn sanitize_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

pub(crate) fn should_dispatch_rename(old: Option<&str>, new: Option<&str>) -> bool {
    old != new
}

pub fn install(
    win: &AppWindow,
    dispatcher: Rc<dyn CommandDispatcher>,
    last_dispatched: Rc<RefCell<Option<String>>>,
) {
    let dispatcher = dispatcher.clone();
    let last_dispatched = last_dispatched.clone();
    win.on_edit_project_name(move |raw| {
        let new = sanitize_name(raw.as_str());
        let mut tracker = last_dispatched.borrow_mut();
        if !should_dispatch_rename(tracker.as_deref(), new.as_deref()) {
            return;
        }
        *tracker = new.clone();
        let _ = dispatcher.dispatch(Command::UpdateProjectName {
            name: new.unwrap_or_default(),
        });
    });
}
```

- [ ] **Step 2: Slint section**

```slint
// crates/adapter-gui/ui/pages/settings/section_project_meta.slint
import { SettingsSection, SettingsScope } from "../../components/settings_section.slint";
import { LineEdit } from "std-widgets.slint";

export component SectionProjectMeta inherits SettingsSection {
    scope: SettingsScope.project;
    title: "Project";

    in property <string> project-name;
    in property <string> project-path-display;
    callback edit-project-name(string);

    VerticalLayout {
        spacing: 8px;
        HorizontalLayout {
            spacing: 8px;
            Text { text: "Name"; color: #cfd5e0; vertical-alignment: center; }
            LineEdit {
                text: root.project-name;
                edited(t) => { root.edit-project-name(t); }
            }
        }
        HorizontalLayout {
            spacing: 8px;
            Text { text: "File"; color: #cfd5e0; vertical-alignment: center; }
            Text {
                text: root.project-path-display;
                color: #8a92a3;
                font-size: 11px;
                vertical-alignment: center;
            }
        }
    }
}
```

Add to `AppWindow`:

```slint
    in property <string> project-name;
    in property <string> project-path-display;
    callback edit-project-name(string);
```

Drop into `SettingsPage`.

- [ ] **Step 3: Wire properties in `lib.rs`**

`project_name` is sourced from the existing project session (already tracked elsewhere — reuse the binding). `project_path_display` is `project_session.path.to_string_lossy()` or `"(unsaved)"` when None.

- [ ] **Step 4: Build + tests + smoke**

```bash
cargo build --workspace -q && cargo test -p adapter-gui project_meta -q
cargo run --bin openrig -- --no-audio
```
Type a new name in the field; confirm `gh log` (or the dispatcher trace) shows `UpdateProjectName` once.

- [ ] **Step 5: Commit + push + issue comment.**

```bash
git add -- crates/adapter-gui/src/settings/project_meta.rs crates/adapter-gui/src/settings/project_meta_tests.rs crates/adapter-gui/ui/pages/settings/section_project_meta.slint crates/adapter-gui/ui/app-window.slint crates/adapter-gui/ui/pages/settings.slint crates/adapter-gui/src/lib.rs
git commit -m "feat(adapter-gui): Project / Metadata section (#513)

Name field auto-saves on edit (dispatches UpdateProjectName, dedup
against the last dispatched value so repeated identical edits do
not flood the bus). File path is read-only — display only, with a
clear '(unsaved)' fallback when the project lives in memory."
git push
```

---

## Task 11: Daemon learn-mode flag + `Event::MidiEventReceived`

**Files:**
- Modify: `crates/adapter-midi/src/daemon.rs`
- Modify: `crates/adapter-midi/src/lib.rs` (export the control channel handle)
- Create: `crates/adapter-midi/src/learn.rs` (tiny state holder)
- Modify: `crates/adapter-midi/src/translate.rs` (add a helper that converts the raw midir bytes into `project::midi::Source` without going through the binding map)

- [ ] **Step 1: Write the failing daemon test**

```rust
// crates/adapter-midi/src/learn_tests.rs (new file)
use super::learn::LearnState;
use std::sync::Arc;

#[test]
fn start_then_stop_returns_to_inactive() {
    let s = Arc::new(LearnState::default());
    assert!(!s.is_active());
    s.start();
    assert!(s.is_active());
    s.stop();
    assert!(!s.is_active());
}

#[test]
fn capturing_one_event_returns_to_inactive() {
    let s = Arc::new(LearnState::default());
    s.start();
    s.on_event_captured();
    assert!(!s.is_active(), "single-shot capture auto-stops");
}
```

- [ ] **Step 2: Implement**

```rust
// crates/adapter-midi/src/learn.rs
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct LearnState {
    active: AtomicBool,
}

impl LearnState {
    pub fn start(&self) { self.active.store(true, Ordering::SeqCst); }
    pub fn stop(&self) { self.active.store(false, Ordering::SeqCst); }
    pub fn is_active(&self) -> bool { self.active.load(Ordering::SeqCst) }
    /// Called once after a single MIDI event has been captured; auto-resets.
    pub fn on_event_captured(&self) { self.stop(); }
}
```

In `crates/adapter-midi/src/lib.rs`:

```rust
pub mod learn;
pub use learn::LearnState;
```

Run:
```bash
cargo test -p adapter-midi learn -q
```
Expected: PASS.

- [ ] **Step 3: Daemon honours the flag**

In `crates/adapter-midi/src/daemon.rs`, the daemon already receives raw MIDI bytes per port and submits resolved `Command`s through the `CommandBridge` (#22 / #165). Inject the `Arc<LearnState>` and a `CommandBridge` clone into the daemon's run signature and the per-port callback. Inside the callback, before binding resolution:

```rust
if learn_state.is_active() {
    if let Some(source) = translate::source_from_bytes(bytes) {
        let _ = bridge.submit(Command::PublishMidiEvent { source });
        learn_state.on_event_captured();
        return;
    }
}
```

`translate::source_from_bytes` is a thin wrapper that returns `Some(project::midi::Source)` for the four supported sources or `None` for noise; lift the parsing already inside `mapping.rs` if duplicated. The daemon already holds the bridge (it uses `bridge.submit(resolved_cmd)` on the normal path), so this introduces no new transport — `PublishMidiEvent` flows through `PublishingDispatcher`'s fan-out and reaches the GUI as `Event::MidiEventReceived` per the dispatcher handler added in task 4.

- [ ] **Step 4: Adapter forwards learn commands to the daemon**

In the adapter wiring that owns the daemon handle (search for `daemon::run` or `Daemon::spawn` in `crates/adapter-gui` or wherever the MIDI thread is constructed in #22 land), subscribe to `Event::MidiLearnStarted` / `Event::MidiLearnStopped` and call `learn_state.start()` / `learn_state.stop()` respectively.

- [ ] **Step 5: Build + test**

```bash
cargo build --workspace -q && cargo test -p adapter-midi -q
```

- [ ] **Step 6: Commit + push.**

```bash
git add crates/adapter-midi/src/learn.rs crates/adapter-midi/src/learn_tests.rs crates/adapter-midi/src/lib.rs crates/adapter-midi/src/daemon.rs crates/adapter-midi/src/translate.rs crates/adapter-gui/src/lib.rs
git commit -m "feat(adapter-midi): single-shot learn-mode flag + MidiEventReceived (#513, #493)

Atomic LearnState shared between the adapter wiring and the daemon
callback. While active, every incoming MIDI event is published as
project::midi::Source through Event::MidiEventReceived and the
flag auto-clears. Off path is unchanged — binding resolution still
runs."
git push
```

---

## Task 12: Project / MIDI mapping section — editor + Learn flow

**Files:**
- Create: `crates/adapter-gui/ui/pages/settings/section_project_midi_mapping.slint`
- Create: `crates/adapter-gui/src/settings/midi_mapping.rs`
- Create: `crates/adapter-gui/src/settings/midi_mapping_tests.rs`
- Modify: `crates/adapter-gui/ui/models.slint` (add `MidiBindingRow`)
- Modify: `crates/adapter-gui/ui/app-window.slint`
- Modify: `crates/adapter-gui/ui/pages/settings.slint`
- Modify: `crates/adapter-gui/src/lib.rs`

- [ ] **Step 1: Add the Slint model**

```slint
export struct MidiBindingRow {
    trigger-label: string,
    command-label: string,
    learning: bool,
}
```

- [ ] **Step 2: Write the failing wiring tests**

```rust
// crates/adapter-gui/src/settings/midi_mapping_tests.rs
use super::midi_mapping::{add_draft, apply_learned_source, finalize_draft, format_trigger};
use project::midi::{Binding, Source};

#[test]
fn format_trigger_program_change() {
    let s = Source::ProgramChange { program: 7 };
    assert_eq!(format_trigger(&s), "PC #7");
}

#[test]
fn format_trigger_note_on() {
    let s = Source::NoteOn { channel: 1, note: 60 };
    assert_eq!(format_trigger(&s), "Note On ch=1 #60");
}

#[test]
fn add_draft_appends_empty_row_in_learning_state() {
    let mut bindings = vec![];
    let mut drafts = vec![];
    add_draft(&mut bindings, &mut drafts);
    assert_eq!(drafts.len(), 1);
    assert!(drafts[0].source.is_none());
    assert!(drafts[0].learning);
}

#[test]
fn apply_learned_source_fills_active_draft_only() {
    let mut drafts = vec![Default::default(), Default::default()];
    drafts[1].learning = true;
    apply_learned_source(&mut drafts, Source::Cc { channel: 1, controller: 7 });
    assert!(drafts[0].source.is_none());
    assert!(matches!(drafts[1].source, Some(Source::Cc { .. })));
    assert!(!drafts[1].learning, "learn auto-stops after capture");
}

#[test]
fn finalize_draft_merges_into_bindings_when_command_chosen() {
    let mut bindings = vec![];
    let draft = super::midi_mapping::Draft {
        source: Some(Source::ProgramChange { program: 5 }),
        command: Some("SaveProject".into()),
        learning: false,
    };
    let ok = finalize_draft(&mut bindings, draft);
    assert!(ok);
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].command, "SaveProject");
}
```

- [ ] **Step 3: Implementation**

```rust
// crates/adapter-gui/src/settings/midi_mapping.rs
use std::cell::RefCell;
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use project::midi::{Binding, Source};

use crate::AppWindow;

#[cfg(test)]
mod midi_mapping_tests;

#[derive(Default, Debug, Clone)]
pub struct Draft {
    pub source: Option<Source>,
    pub command: Option<String>,
    pub learning: bool,
}

pub(crate) fn format_trigger(src: &Source) -> String {
    match src {
        Source::NoteOn { channel, note } => format!("Note On ch={channel} #{note}"),
        Source::NoteOff { channel, note } => format!("Note Off ch={channel} #{note}"),
        Source::Cc { channel, controller } => format!("CC ch={channel} #{controller}"),
        Source::ProgramChange { program } => format!("PC #{program}"),
    }
}

pub(crate) fn add_draft(_bindings: &mut Vec<Binding>, drafts: &mut Vec<Draft>) {
    drafts.push(Draft { source: None, command: None, learning: true });
}

pub(crate) fn apply_learned_source(drafts: &mut [Draft], source: Source) {
    if let Some(d) = drafts.iter_mut().find(|d| d.learning) {
        d.source = Some(source);
        d.learning = false;
    }
}

pub(crate) fn finalize_draft(bindings: &mut Vec<Binding>, draft: Draft) -> bool {
    let (Some(source), Some(command)) = (draft.source, draft.command) else {
        return false;
    };
    bindings.push(Binding { source, command, args: serde_json::Value::Null, scale: None });
    true
}

pub fn install(
    win: &AppWindow,
    dispatcher: Rc<dyn CommandDispatcher>,
    bindings: Rc<RefCell<Vec<Binding>>>,
    drafts: Rc<RefCell<Vec<Draft>>>,
) {
    // on_add_binding, on_delete_binding, on_save_binding, on_cancel_binding,
    // on_pick_command(row_index, command_name) — each mutates state, dispatches
    // SaveMidiMapping with the *finalized* binding list, and re-renders the
    // model. Pattern is identical to midi_devices::install — wire each
    // callback to its matching internal helper.
}
```

(The complete `install()` body mirrors `midi_devices::install` — write each callback closure exactly once; do not factor into a macro.)

- [ ] **Step 4: Slint section**

```slint
// crates/adapter-gui/ui/pages/settings/section_project_midi_mapping.slint
import { SettingsSection, SettingsScope } from "../../components/settings_section.slint";
import { MidiBindingRow } from "../../models.slint";
import { Button, ComboBox, ScrollView } from "std-widgets.slint";

export component SectionProjectMidiMapping inherits SettingsSection {
    scope: SettingsScope.project;
    title: "MIDI mapping";

    in property <[MidiBindingRow]> bindings;
    in property <[string]> available-commands;
    callback add-binding();
    callback delete-binding(int);
    callback pick-command(int, string);
    callback cancel-draft(int);

    VerticalLayout {
        spacing: 8px;
        HorizontalLayout {
            alignment: end;
            Button { text: "+ Add"; clicked => { root.add-binding(); } }
        }
        ScrollView {
            VerticalLayout {
                spacing: 4px;
                for b[i] in root.bindings : Rectangle {
                    background: #1d2025;
                    border-radius: 4px;
                    padding: 8px;
                    HorizontalLayout {
                        spacing: 10px;
                        Text {
                            text: b.learning ? "Press a MIDI control…" : b.trigger-label;
                            color: b.learning ? #ffd166 : #f3f6fb;
                            vertical-alignment: center;
                        }
                        ComboBox {
                            model: root.available-commands;
                            current-value: b.command-label;
                            selected(c) => { root.pick-command(i, c); }
                        }
                        Button {
                            text: b.learning ? "Cancel" : "Delete";
                            clicked => {
                                if (b.learning) {
                                    root.cancel-draft(i);
                                } else {
                                    root.delete-binding(i);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

Add to `AppWindow`:

```slint
    in property <[MidiBindingRow]> midi-bindings;
    in property <[string]> available-commands;
    callback add-midi-binding();
    callback delete-midi-binding(int);
    callback pick-midi-binding-command(int, string);
    callback cancel-midi-binding-draft(int);
```

- [ ] **Step 5: Source `available-commands` from `command_schema`**

```rust
let commands: Vec<SharedString> = application::command_schema::command_variant_names()
    .into_iter()
    .map(SharedString::from)
    .collect();
window.set_available_commands(ModelRc::from(commands.as_slice()));
```

- [ ] **Step 6: Subscribe to `Event::MidiEventReceived` to feed Learn**

In `lib.rs`, after `install`:

```rust
let drafts_for_learn = drafts.clone();
let model_for_learn = midi_binding_model.clone();
event_subscriber.on(EventFilter::MidiEventReceived, move |evt| {
    if let Event::MidiEventReceived { source } = evt {
        let mut d = drafts_for_learn.borrow_mut();
        crate::settings::midi_mapping::apply_learned_source(&mut d, source);
        // re-render model:
        crate::settings::midi_mapping::repaint(&model_for_learn, &bindings.borrow(), &d);
    }
});
```

When a binding is finalized (source + command present), dispatch `Command::SaveMidiMapping { bindings: bindings.clone() }`. Persistence on `Event::MidiMappingSaved` writes the project file via the existing project-save adapter — no new IO code.

- [ ] **Step 7: Build + tests + smoke**

```bash
cargo build --workspace -q && cargo test -p adapter-gui midi_mapping -q
cargo run --bin openrig -- --no-audio
```
Open Settings → MIDI mapping → "+ Add". Wiggle a knob on a connected MIDI controller. Confirm the trigger label fills in. Pick a command. Re-open Settings to confirm the binding persisted.

- [ ] **Step 8: Commit + push + issue comment.**

```bash
git add -- crates/adapter-gui/src/settings/midi_mapping.rs crates/adapter-gui/src/settings/midi_mapping_tests.rs crates/adapter-gui/ui/pages/settings/section_project_midi_mapping.slint crates/adapter-gui/ui/models.slint crates/adapter-gui/ui/app-window.slint crates/adapter-gui/ui/pages/settings.slint crates/adapter-gui/src/lib.rs
git commit -m "feat(adapter-gui): Project / MIDI mapping editor with Learn (#513, #493)

In-app editor for project.midi.bindings — replaces hand-edited YAML
(closes #493). Drafts start in Learn mode; the next published
MidiEventReceived fills the trigger and stops Learn. Picking a
Command from the drop-down finalizes the binding and dispatches
SaveMidiMapping; the project file persists on MidiMappingSaved."
git push
```

---

## Task 13: Documentation update

**Files:**
- Modify: `docs/screens.md`
- Modify: `docs/audio-config.md`
- Modify: `docs/midi.md`

- [ ] **Step 1: `docs/screens.md`**

Replace the "Project settings" entry with a "Settings" entry covering: scope (System vs Project), the five sections, how to add a MIDI binding, where audio devices vs MIDI mapping persist.

- [ ] **Step 2: `docs/audio-config.md`**

Change "open the project settings dialog" → "open the Settings screen, System / Audio interface section".

- [ ] **Step 3: `docs/midi.md`**

- Replace the "edit `midi-bindings.yaml` by hand" instructions with the in-app editor flow.
- Add a "Choosing which MIDI device to listen to" section pointing to System / MIDI devices.
- Add a paragraph on the alias system + the `MidiPortKey { name, instance }` identification rule (one paragraph, link to the spec for the long version).

- [ ] **Step 4: Verify all language files updated together if any of these docs have pt-BR/es-ES companions.**

```bash
grep -rln "midi-bindings.yaml" docs README.md README.pt-BR.md README.es-ES.md
```
Update any remaining references.

- [ ] **Step 5: Commit + push + issue comment.**

```bash
git add docs/screens.md docs/audio-config.md docs/midi.md
git commit -m "docs(#513): update screens, audio-config, midi for Settings screen

Screens.md drops the Project settings entry in favour of a single
Settings screen with explicit scope sections. audio-config and midi
now point users at the in-app screens instead of YAML files. The
MIDI section explains the alias-based device identity model."
git push
```

---

## Task 14: Cleanup — drop deprecated aliases and dead files

**Files:**
- Modify: `crates/infra-filesystem/src/lib.rs` (remove `pub type GuiAudioSettings = GuiSystemSettings`)
- Modify: every call site that still references `GuiAudioSettings` (should be none after task 2 ran clean)
- Modify: `crates/adapter-gui/ui/pages/pages.slint` (drop `ProjectSettingsPage` re-export)
- Delete: `crates/adapter-gui/src/project_settings_wiring.rs` (if its content is fully absorbed; otherwise leave a TODO and reschedule)
- Modify: `crates/infra-filesystem/src/lib_tests.rs` (delete the `gui_system_settings_alias` test from task 2)

- [ ] **Step 1: Grep for remaining references**

```bash
grep -rn "GuiAudioSettings\|ProjectSettingsPage\|project_settings_wiring\|audio_settings_save_wiring\|language_wiring" crates/ docs/ 2>&1 | grep -v "target/"
```
Every match is either:
- already a re-export → remove it now;
- a legitimate reference to something that is not the alias → leave alone.

- [ ] **Step 2: Remove the alias + the back-compat test**

In `crates/infra-filesystem/src/lib.rs`, delete the line:
```rust
pub type GuiAudioSettings = GuiSystemSettings;
```
In `crates/infra-filesystem/src/lib_tests.rs`, delete the `gui_system_settings_alias_resolves_during_deprecation_window` test.

- [ ] **Step 3: Drop the Slint alias**

In `crates/adapter-gui/ui/pages/pages.slint`, remove:
```slint
export { SettingsPage as ProjectSettingsPage }
```

- [ ] **Step 4: Delete now-dead wiring files**

```bash
git rm crates/adapter-gui/src/project_settings_wiring.rs
# audio_settings_save_wiring.rs and language_wiring.rs already moved with git mv in task 8
```

- [ ] **Step 5: Final build + test sweep**

```bash
cargo build --workspace -q
cargo test --workspace -q
cargo clippy --workspace -- -D warnings
cargo fmt --check
```
All four expected to PASS with zero warnings, zero clippy diagnostics, zero format diffs.

- [ ] **Step 6: Commit + push + issue comment.**

```bash
git add -- crates/infra-filesystem/src/lib.rs crates/infra-filesystem/src/lib_tests.rs crates/adapter-gui/ui/pages/pages.slint
git rm crates/adapter-gui/src/project_settings_wiring.rs
git commit -m "chore(#513): drop GuiAudioSettings alias and dead wiring files

The deprecation window closes on this branch — every consumer has
migrated to GuiSystemSettings and the new settings/ module. The
back-compat test and the Slint re-export alias go with it."
git push
```

---

## Task 15: Quality gate + PR

- [ ] **Step 1: Verify the gate (only here, not per push)**

```bash
~/.quality-gate/qg --base origin/develop
```
Expected: green. If anything fails, fix and commit a new chunk (do not amend), push, then re-run.

- [ ] **Step 2: Open the PR**

```bash
gh pr create \
  --title "feat(gui): centralized Settings screen — System + Project + MIDI (#513, closes #493)" \
  --body "$(cat <<'EOF'
## Summary
- Unified Settings screen with five sections under two scope headers (System / Project).
- New MIDI devices section: enumerates with alias-based identity (`MidiPortKey { name, instance }`); persists to `config.yaml`.
- New Project / MIDI mapping editor with single-shot Learn (closes #493); persists to `.openrig`.
- Refactored `ProjectSettingsPage` → `SettingsPage`; per-section `.slint` files + per-section Rust wirings under `crates/adapter-gui/src/settings/`.
- New Commands: `SaveMidiDevices`, `SaveMidiMapping`, `StartMidiLearn`, `StopMidiLearn`. New Events: `MidiDevicesSaved`, `MidiMappingSaved`, `MidiLearnStarted`, `MidiLearnStopped`, `MidiEventReceived`.
- New `adapter_midi::list_input_ports()` with duplicate-name disambiguation; daemon honours a single-shot learn-mode flag.

## Test plan
- [ ] `cargo build --workspace` clean (zero warnings)
- [ ] `cargo test --workspace` green
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] Manual: open Settings, refresh devices, edit alias, restart, confirm persistence
- [ ] Manual: open MIDI mapping, "+ Add", wiggle a knob on a controller, pick a Command, restart, confirm binding persists in `.openrig`
- [ ] Manual: rename project from the Project / Metadata section, save project, reload, confirm name persisted

## Related
- ADR 0003 (`docs/adr/0003-system-vs-project-config.md`)
- Spec: `docs/superpowers/specs/2026-05-21-issue-513-settings-design.md`
- Plan: `docs/superpowers/plans/2026-05-24-issue-513-settings-screen.md`
EOF
)"
```

- [ ] **Step 3: Final issue comment with the PR link and the next-step ask.**

---

## Snippets

### Per-push issue comment template

```bash
gh issue comment 513 --body "$(cat <<EOF
Pushed \`<hash>\` to \`feature/issue-513\`.

Files:
- <one bullet per file>

Build: \`cargo build --workspace -q\` — clean (zero warnings).
Tests: \`cargo test -p <crate> -q\` — <N>/<N> passing.

To check out:
\`\`\`
git fetch && git checkout feature/issue-513 && git pull
\`\`\`
EOF
)"
```
