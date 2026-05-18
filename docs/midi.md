# MIDI / BLE-MIDI controller adapter (#22)

`adapter-midi` lets a physical or wireless controller — a USB or **BLE-MIDI**
footswitch (e.g. the **M-Vave Chocolate**), an expression pedal, an iPad app —
drive the **same `Command`s the GUI uses**, on the same running OpenRig
instance. It is the MIDI sibling of the MCP adapter (`docs/mcp.md`): an opt-in
input that attaches to a live frontend through the command bridge. No
audio-thread code is touched — real-time invariants hold by construction.

## Enabling it

```
openrig --midi              # use the per-OS default midi-map.yaml
openrig --midi=/path/to/map.yaml
```

Same flag on the console runner (`adapter-console`). It can run together with
`--mcp`: both feed the one command bus; the GUI reflects every change live.

Default map location (never hardcoded — same resolver as every OpenRig config):

| OS | Path |
|---|---|
| macOS | `~/Library/Application Support/OpenRig/midi-map.yaml` |
| Windows | `%APPDATA%\OpenRig\midi-map.yaml` |
| Linux | `~/.config/OpenRig/midi-map.yaml` |

If the map is missing or any binding is invalid (unknown command, args that
don't match the command schema) the adapter **refuses to start and logs why** —
bindings are never silently dropped.

## `midi-map.yaml` format

```yaml
# Optional: pick the input device by case-insensitive substring.
# Omit to use the system default input.
input: Chocolate

bindings:
  # Footswitch A (Note On, ch 1, note 60) → toggle a block's bypass
  - source: { kind: note_on, channel: 1, note: 60 }
    command: ToggleBlockEnabled
    args: { chain: "<chain-id>", block: "<block-id>" }

  # Footswitch B (Program Change 5) → save the project
  - source: { kind: program_change, program: 5 }
    command: SaveProject

  # Expression pedal (CC 7, ch 1) → sweep a parameter, 0..127 → 0.0..100.0
  - source: { kind: cc, channel: 1, controller: 7 }
    command: SetBlockParameterNumber
    args: { chain: "<chain-id>", block: "<block-id>", path: gain }
    scale: { min: 0.0, max: 100.0 }
```

- `source.kind` is one of `note_on`, `note_off`, `cc`, `program_change`.
  `channel` is **1..=16** (human numbering). `program_change` ignores channel.
- `command` is a `Command` **variant name** (PascalCase, exactly as in the
  enum). `args` is that variant's argument object. Both are validated against
  the same auto-derived schema the MCP tools use — one source of truth.
- `chain`/`block` are the project's **string ids**, not ordinals. Read them
  from the current project (`openrig://project` over MCP, or `project.yaml`).
- `scale` (continuous sources only) maps the incoming 0..=127 linearly into
  `[min, max]` and writes it into the argument named `into` (default `value`).
  A `cc` binding without `scale` passes the raw 0..=127 as `value`.
- First matching binding wins.

## M-Vave Chocolate (BLE-MIDI) — worked example

1. Pair the Chocolate over Bluetooth at the OS level (macOS:
   *Audio MIDI Setup → MIDI Studio → Bluetooth*; the device then appears as a
   normal MIDI input — no OpenRig-specific step).
2. Drop this at the default `midi-map.yaml` path (the Chocolate's four
   switches default to Program Change / CC; adjust `program`/`note` to your
   unit's mode):

```yaml
input: Chocolate
bindings:
  - source: { kind: program_change, program: 0 }
    command: LoadProject
    args: { path: presets/clean.yaml }
  - source: { kind: program_change, program: 1 }
    command: LoadProject
    args: { path: presets/crunch.yaml }
  - source: { kind: note_on, channel: 1, note: 60 }
    command: ToggleBlockEnabled
    args: { chain: "<chain-id>", block: "<delay-block-id>" }
  - source: { kind: cc, channel: 1, controller: 11 }
    command: SetBlockParameterNumber
    args: { chain: "<chain-id>", block: "<amp-block-id>", path: gain }
    scale: { min: 0.0, max: 100.0 }
```

3. Run `openrig --midi`. The log prints the matched input port. Footswitches
   now drive the live rig; a knob moved in the GUI is still visible to MCP and
   vice-versa (one shared `ProjectSession`).

## Scope (v1)

In: USB-MIDI + BLE-MIDI input, the YAML mapping above, Note/CC/Program Change,
linear scale. Out (follow-ups): a mapping editor UI, per-project maps, MIDI
**output** / LED feedback to the controller, log scaling, hot-reload, OSC
(behind a future Cargo feature).

## Design

`docs/superpowers/specs/2026-05-18-22-midi-osc-adapter-design.md` — realizes
Phase 3 of `docs/superpowers/specs/2026-04-23-command-dispatch-architecture-design.md`.
