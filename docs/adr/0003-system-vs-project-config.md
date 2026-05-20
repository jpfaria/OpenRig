# ADR 0003: System vs Project Configuration

## Status

Accepted

## Context

OpenRig has accumulated two persisted configuration surfaces with an implicit ownership
rule between them:

- `config.yaml` (per-OS config dir) — recent projects, asset paths, GUI audio settings,
  language. Treated as global/app-level.
- `project.openrig` ([`RigProject`](../projects/project-openrig-format.md)) — chains,
  blocks, devices, scenes. Treated as project-level.
- `midi-map.yaml` (per-OS config dir, ADR for #22) — a single global controller mapping
  shared by every project. Per-project mapping was explicitly **out of scope for v1**
  of #22.

New settings have surfaced without a written rule for which surface they belong to:

- #493 — MIDI mapping inside the project + in-app editor — collides with the global
  `midi-map.yaml`.
- Interface preferences (theme, window state, layout) — undecided home.

Without a rule, each new setting requires a fresh debate and the next file migration.

## Decision

A setting belongs to **PROJECT** if the answer to *"if I send this `.openrig` to another
machine, does this value have to travel with it?"* is **yes**. Otherwise it belongs to
**SYSTEM**.

- **System config** = belongs to the installation / machine / user. Lives in
  `config.yaml` (per-OS config dir). Same person, same value, regardless of which project
  is open.
- **Project config** = belongs to the rig / setlist. Lives inside `project.openrig`.
  Travels with the file when the user moves it to another machine.
- **Precedence at load time** = project overrides system where both can describe the
  same dimension. System provides defaults; project pins.

### Classification (current + planned settings)

**System (`config.yaml`):**

- UI language / locale.
- Theme, window size / position, last screen, generic interface preferences that belong
  to the user, not to the rig.
- Recent projects, asset paths.
- Audio device defaults — physical device IDs are machine-specific.
  See [Sub-question](#sub-question--device-settings-tension) below; this ADR does not
  relitigate ADR 0001.
- **MIDI device profile** — describes *which* controller to listen to (input port
  substring match). Belongs to the machine because it's the user's hardware.

**Project (`project.openrig`):**

- Chains / blocks / presets / scenes (already there).
- Project-scoped interface layout that is part of the rig (e.g. a pedalboard arrangement
  inseparable from the setlist).
- **MIDI binding map** — describes *what this rig does* when a binding fires. Travels
  with the project so the same setlist behaves identically on any machine.

### MIDI two-layer split

`midi-map.yaml` (single global file) is split into:

1. **MIDI device profile** — `input: Option<String>` only. Lives in
   `~/.config/OpenRig/midi-profile.yaml` (and per-OS equivalents). One per machine.
2. **MIDI binding map** — `bindings: Vec<Binding>`. Lives under
   `RigProject.midi.bindings` inside `project.openrig`. Travels with the project. A
   fallback `~/.config/OpenRig/midi-bindings.yaml` provides system-wide defaults when a
   project has no `midi:` field; the shipped `examples/midi-map.default.yaml` is the
   ultimate fallback.

### Migration / backward compatibility

The original `midi-map.yaml` (input + bindings) is split on first load:

- `input:` is moved to `midi-profile.yaml` (system).
- `bindings:` is moved to `midi-bindings.yaml` (system fallback).
- The original `midi-map.yaml` is deleted after a successful split.

Projects saved before this ADR have no `midi:` field, parse cleanly, and use the system
fallback / shipped default — audio and behavior are identical.

### Load-time resolution

```
resolve_midi(project, system_profile, fallback_bindings):
    input    = system_profile.input            (project never overrides)
    bindings = project.midi.bindings           (when set)
               || fallback_bindings.bindings   (when present)
               || shipped_default.bindings     (always present)
```

`input` is intentionally not overridable by the project: the same hardware controller is
used regardless of which rig is loaded.

## Out of scope

- ADR 0001 (project model) is **not** reopened to revisit whether `device_settings`
  belongs at project or system level. Tracked separately if revisited.
- Cloud sync / multi-user project sharing.

## Sub-question — `device_settings` tension

ADR 0001 places `device_settings` inside `project.yaml`. Physical device IDs are
machine-specific, which fails the "travels with the project" test on the surface.
The ADR is preserved as-is for now because:

- The current model uses the device ID as a routing target, not as a portability
  guarantee.
- Reworking this requires a parallel system-level defaults layer plus a logical→physical
  resolution step, which is its own decision.

A future ADR may revisit this; #499 documents the rule so the discussion can happen with
the principle written down.

## Consequences

- Future settings have a written test for where they live.
- MIDI mapping per project becomes possible (closes #493 as superseded or reframes it as
  the in-app editor for the project layer).
- Migrating a `midi-map.yaml` from before #499 happens silently on first load; no user
  action.
- Older `project.openrig` files remain valid because `midi:` defaults to `None`.
