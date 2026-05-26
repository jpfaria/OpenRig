# MIDI profiles (issue #548)

OpenRig ships a **per-controller profile** system: plug a known
footswitch / knob box / expression pedal, pick its profile in
**Settings → MIDI**, play. No manual YAML editing. Multiple profiles
active simultaneously is the normal case (an FCB1010 + an expression
pedal + a Chocolate Plus, all at once).

For the design rationale and the 10-phase plan, see
[`docs/superpowers/specs/2026-05-26-midi-profiles-design.md`](superpowers/specs/2026-05-26-midi-profiles-design.md).

## Profile file format

A profile lives in `assets/midi-profiles/<name>.yaml` (factory,
bundled) or `~/.local/share/openrig/midi-profiles/<name>.yaml` (user,
created via Settings → MIDI → [Customize]). Each YAML has a matching
`<name>.md` next to it with the human-readable bank/switch table.

```yaml
name: "Chocolate Plus — Program Change A (factory)"
source: "FootCtrlPlus"     # optional substring of the MIDI port name
description: |
  M-Vave Chocolate Plus in CubeSuite "Program change A" mode.

bindings:
  - when: { kind: ProgramChange, channel: 1, program: 0 }
    do: prev_preset
  - when: { kind: ProgramChange, channel: 1, program: 1 }
    do: next_preset
  - when: { kind: ControlChange, channel: 1, controller: 7 }
    do: chain_volume
```

- `name` is shown in the Settings list.
- `source` (optional) filters by **substring** of the MIDI port name —
  e.g. `"FootCtrlPlus"` matches `"FootCtrlPlus Bluetooth"` on macOS
  and `"FootCtrlPlus USB"` on Linux. Omit it for a profile that
  applies to every MIDI port.
- `bindings` is the action list.
- Each binding has a **`when`** (MIDI message pattern) and a **`do`**
  (slot name from the 20-slot catalog below). Slot names not in the
  catalog are rejected at parse time.

### `when` shape

`kind` uses MIDI 1.0 standard names. Value field name follows the
message type:

| `kind`          | Required field | Optional value field |
|-----------------|----------------|----------------------|
| `NoteOn`        | `channel`      | `note`               |
| `NoteOff`       | `channel`      | `note`               |
| `ControlChange` | `channel`      | `controller`         |
| `ProgramChange` | `channel`      | `program`            |

`channel` is `1–16`. Value is `0–127`. **Omit the value field to
wildcard** — match any byte. Used by `jump_preset_n` and
`jump_scene_n` (the byte becomes the action's index) and by continuous
CC slots (`chain_volume`, `block_param_numeric`).

### `do` — the 20-slot catalog

| Slot | Group | Acts on | Effect |
|---|---|---|---|
| `toggle_tuner` | App | global | flips the tuner button |
| `toggle_output_mute` | App | global | flips output mute |
| `toggle_spectrum` | App | global | flips the spectrum window |
| `prev_chain` | Chain nav | active chain | select previous chain (wraps) |
| `next_chain` | Chain nav | active chain | select next chain (wraps) |
| `toggle_active_chain_enabled` | Chain | active chain | enable/disable the active chain |
| `toggle_compact_view` | Chain | active chain | flip the compact-view UI |
| `prev_preset` | Rig nav | active chain | previous preset (wraps) |
| `next_preset` | Rig nav | active chain | next preset (wraps) |
| `prev_scene` | Rig nav | active chain | previous scene (wraps) |
| `next_scene` | Rig nav | active chain | next scene (wraps) |
| `jump_preset_n` | Rig nav | active chain | jump to preset `value` (MIDI value byte) |
| `jump_scene_n` | Rig nav | active chain | jump to scene `value` |
| `prev_block_1` | Block nav | active block | one block back (wraps) |
| `next_block_1` | Block nav | active block | one block forward (wraps) |
| `prev_block_2` | Block nav | active block | two blocks back (for compact view) |
| `next_block_2` | Block nav | active block | two blocks forward |
| `toggle_active_block_enabled` | Block | active block | enable/disable the active block |
| `chain_volume` | Continuous CC | active chain | set chain volume from CC value (scaled) |
| `block_param_numeric` | Continuous CC | active block | set the active block's first numeric param from CC value |

"Active chain" / "active block" come from `SelectionState` —
synchronised with what the user has selected on the Chains screen.
Slots that need an active chain/block and there is none simply do
nothing (a footswitch press on a not-yet-loaded project is a no-op).

## Architecture

Match flow on a raw MIDI byte:

```
raw bytes
   ↓
IncomingMessage   (kind, channel, value-byte)
   ↓
pipeline::match_message(active_profiles, port_name, msg)
   ↓
[SlotHit { slot, message }, …]   one entry per binding that fired
   ↓
slots::slot_to_command(slot, message, selection)
   ↓
LocalDispatcher.dispatch(command)
   ↓
GUI + MCP + gRPC all see the same Event
```

Two profiles binding the same message both fire — by design. The user
asks for them by activating both; we don't second-guess.

## Adding a profile

1. Open the controller's vendor app (CubeSuite for the M-Vave
   Chocolate family). Configure it the way you want; export the
   mapping if the app supports it.
2. Capture what each switch / knob actually sends — on macOS use
   [MIDI Monitor](https://www.snoize.com/midimonitor/); on Linux,
   `receivemidi` (Homebrew `gbevin/tools/receivemidi`).
3. Write `<name>.yaml` in `assets/midi-profiles/`. The 20-slot list
   above is the closed set — pick from it.
4. Write a matching `<name>.md` with the bank/switch table.
5. The Phase 2 parser will reject unknown slots and malformed shapes
   at load time, before the profile ever ships.

There's a forthcoming `openrig-midi-profile-builder` skill in the
`openrig` plugin (Phase 8) that walks you through this interactively.

## Currently shipped profiles

- [Chocolate Plus — Program Change A (factory)](../assets/midi-profiles/chocolate_plus_program_change_a.md)

## Activating profiles at runtime

The MIDI daemon that ties everything together lives in `adapter-midi`:

```rust
use adapter_midi::spawn_with_bundled_profiles;

let _handle = spawn_with_bundled_profiles(
    bridge,                    // CommandBridge clone
    dispatcher.selection_state(), // Arc<RwLock<SelectionState>>
    learn,                     // Arc<LearnState>
);
```

That single call opens every available MIDI input, loads every YAML in
`assets/midi-profiles/` (baked in via `include_str!`), and starts
routing incoming messages through the pipeline. Each MIDI message is
matched against every active profile; matches dispatched to the
bridge; the GUI's drain loop runs them through the dispatcher exactly
like a click would.

The GUI also has to **mirror its selection into `SelectionState`** —
when the user clicks a chain or a block, the same `Arc<RwLock<_>>` the
daemon reads must reflect that. The dispatcher already exposes the
handle (`LocalDispatcher::selection_state()`); writing to it on click
is the GUI integrator's responsibility (one writeline per click
callback).

Both pieces are still pending in adapter-gui at the time of writing —
the daemon function and the bundled loader are ready; the GUI's `main`
hasn't been switched off the legacy `run_blocking_with_map` path yet.
That switch + the selection-mirror is the last thing standing between
this issue and a working footswitch.
