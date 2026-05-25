# Centralized Settings Screen — Design (#513)

**Date:** 2026-05-21
**Issue:** #513 (also closes #493 — MIDI mapping editor)
**Branch:** `feature/issue-513` (from `develop`)
**Status:** Approved design — pending implementation plan

Unifies the three configuration surfaces of OpenRig (system, project, MIDI) into a
single Settings screen, and gives MIDI device selection + MIDI mapping their
first GUI. Ratifies ADR 0003 in the UI: every section is labelled with its
storage scope so users can see at a glance what travels with the `.openrig`
file and what stays on the machine.

## Goal

A single Settings screen in the desktop GUI with five sections:

1. **System / Audio interface** — input/output devices, sample rate, buffer size.
2. **System / Language** — UI language (existing).
3. **System / MIDI devices** — enumerate connected MIDI inputs, enable/disable per device, edit a user-facing alias per device. No device roles: what each control does is decided in the mapping editor, not on the device row.
4. **Project / Metadata** — project name (existing `UpdateProjectName`) and the current project file path (read-only display; "Reveal in file manager" affordance optional).
5. **Project / MIDI mapping** — in-app editor for CC/PC/Note → `Command` bindings, with single-shot "MIDI Learn" capture; replaces hand-edited `midi-bindings.yaml`.

Sections 1, 2, 3 persist to `config.yaml` (per-machine). Sections 4, 5 persist
to the active `.openrig` (per-project), per ADR 0003.

All edits flow through `Command`s — no `borrow_mut()` in callbacks. The GUI
stays a pure dispatcher.

**Save model:** every section auto-saves on each user edit. There is no global
"Save" button on the screen — closing the screen never loses an edit.

## Non-goals (v1)

- Visual redesign of the screen beyond what the new sections require.
- MIDI **output** device selection (the daemon is input-only today; tracked in #22 follow-ups).
- BLE-MIDI pairing UI (covered by #297).
- Sample-rate / buffer-size overrides if not already present in `GuiAudioSettings` — keep current fields, do not expand.
- A "Save All" global button. Each section saves independently when changed.
- A new entry-point or top-bar redesign — reuse the existing entry that opens `ProjectSettingsPage` today.

## Context (what already exists — do not rebuild)

- `crates/adapter-gui/ui/pages/project_settings.slint` (340 LOC) — current page; covers audio devices + language. Misnamed: it holds *system* settings, not project settings.
- `crates/adapter-gui/src/project_settings_wiring.rs`, `audio_settings_save_wiring.rs`, `language_wiring.rs` — wirings for the current page.
- `crates/application/src/command.rs` — `Command::SaveAudioSettings`, `Command::UpdateProjectName`, `Command::SetLanguage` already exist and are dispatched today.
- `crates/infra-filesystem/src/lib.rs`:
  - `GuiAudioSettings` (input/output devices, per-machine, in `config.yaml`).
  - `app_config_path()` resolves `config.yaml` per OS (macOS `~/Library/Application Support/OpenRig/config.yaml`, Windows `%APPDATA%\OpenRig\config.yaml`, Linux `~/.local/share/openrig/config.yaml`).
  - `load_gui_audio_settings()` / `save_gui_audio_settings()`.
  - `midi-profile.yaml` path resolver (already present per ADR 0003 / #499).
- `crates/adapter-midi/` — `midir`-based daemon. Has `resolve_midi_map` (project → system fallback → shipped default). No port enumeration API exposed today; the daemon opens ports internally.
- `desktop_main.slint:482` — entry via `show-project-settings` boolean from the top bar.

Conclusion: this is mostly a refactor and an extension. We keep the existing
wirings working (audio + language) and bolt on Project + MIDI sections. No
new top-bar entry, no new transport, no new dispatcher.

## Architecture

### Slint file layout

```
crates/adapter-gui/ui/pages/
  settings.slint                       # renamed from project_settings.slint; container + scope headers
  settings/
    section_system_audio.slint         # extracted from current page
    section_system_language.slint      # extracted from current page
    section_system_midi_devices.slint  # new
    section_project_meta.slint         # new
    section_project_midi_mapping.slint # new
  pages.slint                          # re-export SettingsPage (alias keeps ProjectSettingsPage during one release for back-compat)
```

A new component `SettingsSection` (under `ui/components/settings_section.slint`)
provides the visual frame (scope badge "System" / "Project", title, separator).
Each section .slint file declares its own properties/callbacks; the container
only routes.

Cap each new section file at ~200 LOC (per `docs/development/file-organization.md`).

### Rust module layout

```
crates/adapter-gui/src/settings/
  mod.rs                       # wires all sections to the SettingsPage component
  audio.rs                     # moved from audio_settings_save_wiring.rs
  language.rs                  # moved from language_wiring.rs
  midi_devices.rs              # new — enumerate + persist
  project_meta.rs              # new — UpdateProjectName dispatch + save state
  midi_mapping.rs              # new — bindings list, learn mode, persistence
```

Old files (`project_settings_wiring.rs`, `audio_settings_save_wiring.rs`,
`language_wiring.rs`) become thin re-exports for one cycle, then are removed
in a follow-up cleanup commit on the same branch.

### Data model

Extend the existing system-config struct in `infra-filesystem`:

```rust
// renamed from GuiAudioSettings (alias kept for one cycle)
pub struct GuiSystemSettings {
    pub input_devices: Vec<GuiAudioDeviceSettings>,
    pub output_devices: Vec<GuiAudioDeviceSettings>,
    pub language: Option<String>,
    pub midi_devices: Vec<MidiDeviceSelection>, // NEW
}

pub struct MidiDeviceSelection {
    /// Stable identity slot. See "MIDI device identification" below.
    pub port_key: MidiPortKey,
    /// User-facing label. Defaults to the OS-reported port name on first
    /// detection; editable from the GUI.
    pub alias: String,
    pub enabled: bool,
}
```

No `MidiRole` enum: device rows are storage + identity only. Behaviour
(what each CC/Note triggers) lives in the project's MIDI mapping.

Project-side, the project schema already carries `midi.bindings` (per #499 /
ADR 0003). The new editor reads/writes the same field — no schema change.

### Commands (new vs reused)

Reused:
- `Command::SaveAudioSettings { device_settings }` — unchanged.
- `Command::UpdateProjectName { name }` — unchanged.
- `Command::SetLanguage { language }` — unchanged.

New:
- `Command::SaveMidiDevices { devices: Vec<MidiDeviceSelection> }` — persist to `config.yaml` and notify the daemon to (re)open enabled ports.
- `Command::SaveMidiMapping { bindings: Vec<MidiBinding> }` — persist to the project file under `midi.bindings`.
- `Command::EnumerateMidiDevices` — query-style; produces `Event::MidiDevicesEnumerated { ports: Vec<MidiPortInfo> }`. The wiring drains the bridge for the response and updates the UI model.
- `Command::StartMidiLearn` — puts the daemon into single-shot capture; the next valid MIDI event publishes `Event::MidiLearnCaptured { trigger: MidiTrigger }` and the daemon returns to normal mode automatically. Cancellable by dispatching it again with `cancel: true` (or by a second call from the wiring on UI close).

Each new variant is added to `Command`, `Event`, `LocalDispatcher`, and the
adapter-mcp/adapter-midi parity tables in the same commit that introduces it,
per the architecture law in CLAUDE.md.

### MIDI device enumeration

Add a pure function to `adapter-midi`:

```rust
pub struct MidiPortInfo {
    pub key: MidiPortKey,
    pub raw_name: String,    // exact OS-reported name
}
pub fn list_input_ports() -> anyhow::Result<Vec<MidiPortInfo>>;
```

Uses `midir::MidiInput::new(...)?.ports()` and resolves each port to a
`MidiPortKey` via the identification rule below. The daemon already
constructs an enumerator client — that path is extracted into the new
function and the daemon calls it too.

The "Refresh" button in the section dispatches `EnumerateMidiDevices`; the
side-effect runs `list_input_ports()` off-thread (already off-thread today —
the daemon does enumeration in its dedicated thread). Result returns as
`Event::MidiDevicesEnumerated`.

### MIDI device identification (`MidiPortKey`)

The user requirement is: two physically distinct devices, even with the
same OS-reported name, must be addressable separately. midir does not
expose a stable hardware id cross-platform, so the key is built in two
layers and stored as a typed value:

```rust
pub struct MidiPortKey {
    /// Raw OS-reported port name (always present).
    pub name: String,
    /// Disambiguator when several ports share the same `name`. Starts at
    /// 1 and is assigned in enumeration order at first detection. Zero
    /// (the default) means "only one port with this name on this machine".
    pub instance: u32,
}
```

Enumeration rule (in `list_input_ports`):

1. Read raw port names from midir, preserving discovery order.
2. Group by name. Within each group, assign `instance = 1, 2, 3, …` in
   discovery order.
3. Look up each `MidiPortKey` in the persisted `midi_devices` list:
   - found → keep stored `alias` and `enabled`;
   - not found → seed a row with `alias = name` (or `"{name} (#{instance})"` if `instance > 1`) and `enabled = false`.
4. Persist the seeded rows back into `config.yaml` so the user can rename
   them without re-detection.

This guarantees: a single unique device always gets `instance = 0` and
its row survives reboots / OS quirks via its name. Two identical devices
keep stable per-machine keys as long as their detection order is
consistent — and the user-edited alias makes the visual identity
unambiguous regardless.

Platform-native unique ids (CoreMIDI `uniqueID`, ALSA `client:port`,
Windows MME device index) are **not** used in v1: midir does not surface
them, and pulling them in requires per-platform crates. Tracked as a
follow-up if the name+instance scheme proves insufficient in practice.

### MIDI mapping editor

Slint UI:
- Table of bindings: trigger summary (e.g. `CC ch=1 #64`), command summary, `Edit`, `Delete`.
- `+ Add binding` button: opens an inline row with a "Listen" toggle ("MIDI Learn") + a command dropdown.
- "Listen" toggle dispatches `Command::StartMidiLearn` (new) which puts the daemon into a single-shot capture mode; the next valid MIDI event is published as `Event::MidiLearnCaptured { trigger }` and the wiring fills the trigger field.

Reuse the existing `MidiTrigger` and `MidiBinding` types from `adapter-midi`.
The command dropdown is populated by introspecting the `Command` enum filtered
to the `mappable: bool` flag (already used by adapter-mcp for tool-surface
gating).

Persistence: `Command::SaveMidiMapping` writes the full binding list into
`project.openrig` under `midi.bindings`, replacing whatever was there. The
project save path is unchanged.

### Navigation

No change. The top-bar entry that today sets `show-project-settings = true`
continues to open the (now renamed) `SettingsPage`. The page title in the
header switches from "Project settings" to "Settings".

The `show-project-settings` boolean and the `close-project-settings` /
`save-audio-settings` callbacks on `AppWindow` are renamed to
`show-settings`, `close-settings`, `save-system-settings` in the same commit
that renames the page. There is only one caller (desktop_main.slint), so the
rename is a closed change.

## Persistence flow

```
section change ─► wiring dispatches Command
                  └─► dispatcher updates state + queues SideEffect
                       └─► adapter persists (config.yaml or .openrig)
                            └─► Event fan-out tells the UI to refresh
```

No section writes to disk directly. No section calls `Filesystem::*` from a
callback. The wirings only build the `Command` payload.

## Testing (TDD red-first)

Every new behaviour starts with a failing test. Tests live next to the unit
they cover (per `docs/testing.md`).

### Wiring tests (`*_tests.rs` for each new wiring)

- `midi_devices.rs`: toggling `enabled` or editing `alias` dispatches `SaveMidiDevices` with the expected list. Refresh button dispatches `EnumerateMidiDevices`. Receiving `Event::MidiDevicesEnumerated` merges newly-seen ports without losing user-edited aliases for ports that disappeared and reappeared.
- `midi_mapping.rs`: `+ Add binding` produces a draft row that does NOT dispatch until "Save". "Listen" + `Event::MidiLearnCaptured` fills the trigger field. "Save" dispatches `SaveMidiMapping` with the full list, including the new binding.
- `project_meta.rs`: name edit dispatches `UpdateProjectName` (debounced — confirm against the existing project-name flow before locking the debounce in the test).

### Dispatcher tests (`local_dispatcher_tests.rs`)

- `SaveMidiDevices` updates `GuiSystemSettings.midi_devices` and produces a `SideEffect::PersistSystemSettings`.
- `EnumerateMidiDevices` produces a `SideEffect::EnumerateMidi` (no state mutation).
- `SaveMidiMapping` updates the active project's `midi.bindings` and produces `SideEffect::PersistProject`.

### Filesystem tests (`infra-filesystem/src/lib_tests.rs`)

- Saving a `GuiSystemSettings` with `midi_devices` populated round-trips through `config.yaml`.
- A `config.yaml` written by the previous schema (no `midi_devices` field) loads cleanly with `midi_devices: vec![]`.
- The `GuiAudioSettings` alias still resolves for one cycle (back-compat smoke test).

### Adapter-midi tests

- `list_input_ports()` returns the same `Vec<MidiPortInfo>` shape on macOS, Windows, Linux given a mock midir backend (or feature-gated platform integration test).
- Two ports with identical name produce `instance = 1` and `instance = 2` in discovery order; a single port produces `instance = 0`.
- The single-shot learn capture path: feed one synthetic MIDI event into the daemon in learn mode, expect exactly one `Event::MidiLearnCaptured` and an automatic return to normal mode.

### Slint render tests

None new. The existing page already renders in the integration smoke; the
extracted sections inherit that coverage.

## Acceptance criteria

- The Settings screen opens from the existing top-bar entry and shows five sections grouped under two scope headers ("System" / "Project").
- The audio interface and language sections behave identically to today (no regression — existing tests must still pass without modification beyond rename).
- The MIDI devices section lists connected input ports, lets the user toggle `enabled` per device, lets the user edit an alias per device, and persists to `config.yaml`. Two physically distinct devices with the same OS-reported name appear as separate rows (`instance` 1 and 2) and can carry different aliases. Closing and reopening the app preserves selections and aliases.
- The MIDI mapping section lets the user add a binding via MIDI Learn, edit it, delete it, and persists to `.openrig`. Moving the `.openrig` to another machine carries the mapping with it; the device selection does NOT carry over.
- Every state change flows through a `Command`. No callback calls `borrow_mut` on the model or writes to disk directly.
- `cargo build --workspace` is clean. No new warnings.
- New tests are red before the implementation, then green after.

## Risks & mitigations

- **midir port id stability** — midir does not give us a stable hardware id on all platforms. Mitigation: `MidiPortKey { name, instance }` plus a user-editable `alias` (see "MIDI device identification"). Detection-order quirks (same two devices reported in swapped order across reboots) would scramble `instance`; mitigation is the alias the user sets — the user knows which one is which by name, and re-enabling on the swapped row is one click. Platform-native unique ids are a follow-up if this proves insufficient.
- **Renaming `GuiAudioSettings` → `GuiSystemSettings`** — there are several call sites (see grep in Context). Mitigation: rename the struct, keep a `pub type GuiAudioSettings = GuiSystemSettings` alias for one cycle, deprecate, remove in a follow-up commit on the same branch before PR.
- **PR size** — covers two issues. Mitigation: commit per section (audio rename → language move → midi devices → project meta → midi mapping → cleanup). Each commit compiles and passes its own tests so review can step through.

## Out of scope (explicit)

- The `desktop-pedal` redesign (#398) and the touch UI (#39, #126) — the new sections render inside the existing desktop layout only. Touch parity tracked in a follow-up.
- MIDI output / LED feedback to controllers — input only (per `2026-05-18-22-midi-osc-adapter-design.md` non-goals).
- A separate "remote / network" tab — out of scope; will land when the gRPC transport (#42, #127) needs a settings surface.

## Related

- ADR 0003 — `docs/adr/0003-system-vs-project-config.md`
- #255 (CLOSED) — `device_settings` removed from project schema
- #499 (CLOSED) — system vs project config taxonomy umbrella (includes `midi-profile.yaml` / `midi-bindings.yaml` split)
- #493 — MIDI mapping in-project + in-app editor (closed by this work)
- #22 — MIDI/OSC adapter (consumes the device selection this screen produces)
- #504 — PROJECT / CHAIN / PRESET / SCENE lifecycle (companion; orthogonal)
- #511 — Replace native OS dialogs with in-app Slint dialogs (the "Refresh devices" feedback and the "Delete binding" confirmation should follow this)
