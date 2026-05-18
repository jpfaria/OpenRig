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

## Footswitch commands (#22)

Any `Command` can be bound, but these are the ones built for live
footswitch use. A footswitch press carries no value, so these are
**relative / stepping** and the dispatcher owns the state — pressing a
switch moves the screen **and** the live runtime exactly like a mouse
click (same refresh path).

| Action | `command` | `args` |
|---|---|---|
| Next preset | `ApplyRigNav` | `{ chain: "rig:<input>", kind: { StepPreset: 1 } }` |
| Previous preset | `ApplyRigNav` | `{ chain: "rig:<input>", kind: { StepPreset: -1 } }` |
| Next scene | `ApplyRigNav` | `{ chain: "rig:<input>", kind: { StepScene: 1 } }` |
| Previous scene | `ApplyRigNav` | `{ chain: "rig:<input>", kind: { StepScene: -1 } }` |
| Go to a fixed preset / scene | `ApplyRigNav` | `{ chain: "rig:<input>", kind: { Preset: <pos> } }` (or `Scene`) |
| Move block selection (pair) forward | `SelectChainBlock` | `{ chain: "rig:<input>", delta: 2 }` |
| Move block selection back | `SelectChainBlock` | `{ chain: "rig:<input>", delta: -2 }` |
| Toggle the **left** block of the pair | `ToggleSelectedBlock` | `{ chain: "rig:<input>", side: Left }` |
| Toggle the **right** block of the pair | `ToggleSelectedBlock` | `{ chain: "rig:<input>", side: Right }` |
| Toggle a whole chain on/off | `ToggleChainEnabled` | `{ chain: "<chain-id>" }` |
| Toggle one fixed block on/off | `ToggleBlockEnabled` | `{ chain: "<chain-id>", block: "<block-id>" }` |
| Save the project | `SaveProject` | *(none)* |

Notes:

- Preset/scene stepping **wraps** (after the last comes the first).
- The block-selection pair is two adjacent blocks; `delta: 2` walks it
  two-by-two. The on-screen border appears on the footswitch press and
  fades out shortly after (it is a transient cue, not a persistent
  selection).
- `rig:<input>` is the chain id of a rig input on the chains screen.
- `Chocolate Plus` lets you set the MIDI **channel per message** in its
  editor, so one pedal switching banks (or several pedals) can target
  different actions by channel.

### Example — M-Vave Chocolate, 4 switches as Note on ch 1

Pedal: each switch set to **Note**, channel **1**, notes **60/61/62/63**.

```yaml
input: Chocolate
bindings:
  - source: { kind: note_on, channel: 1, note: 60 }   # A: previous preset
    command: ApplyRigNav
    args: { chain: "rig:guitar", kind: { StepPreset: -1 } }
  - source: { kind: note_on, channel: 1, note: 63 }   # D: next preset
    command: ApplyRigNav
    args: { chain: "rig:guitar", kind: { StepPreset: 1 } }
  - source: { kind: note_on, channel: 1, note: 61 }   # B: toggle left of pair
    command: ToggleSelectedBlock
    args: { chain: "rig:guitar", side: Left }
  - source: { kind: note_on, channel: 1, note: 62 }   # C: toggle right of pair
    command: ToggleSelectedBlock
    args: { chain: "rig:guitar", side: Right }
```

(Swap any binding for `SelectChainBlock` / `StepScene` to taste — the
pedal only sends the note number; the meaning lives here, change it
anytime without touching the pedal.)

## Connecting the M-Vave Chocolate — step by step

The M-Vave Chocolate (and Chocolate Plus) is a 4-switch **BLE-MIDI**
footswitch. OpenRig needs no driver: once the OS pairs it, it is just another
MIDI input and the steps below are the whole setup.

### 1. Know what the switches send

Each switch can be set to send **Program Change**, **Control Change (CC)** or
**Note**, per channel, using the free **MVAVE MIDI** app (iOS/Android) — pair
the pedal there, open *Footswitch settings*, and read off, per switch:

- the **message type** (PC / CC / Note),
- the **number** (program / controller / note),
- the **MIDI channel** (1–16).

Common factory default: the four switches send **Program Change 0..3 on
channel 1**. Whatever you see in the app is exactly what goes into the map
(`kind`, the number, `channel`). You don't need to change the pedal — you
mirror its config in `midi-map.yaml`.

> Tip: leave the MVAVE app's monitor open while you press switches to confirm
> the exact message before writing the binding.

### 2. Pair the pedal with your computer

The pedal must be a system MIDI input. Hold the pairing switch combo until the
LED blinks (see the pedal manual), then:

- **macOS** — open *Audio MIDI Setup* → menu *Window ▸ Show MIDI Studio* →
  *Bluetooth* icon → **Connect** next to "Chocolate". It now shows up as a
  MIDI source.
- **Windows** — *Settings ▸ Bluetooth & devices ▸ Add device ▸ Bluetooth*,
  pick "Chocolate". Windows 10/11 exposes paired BLE-MIDI devices to apps
  automatically.
- **Linux** — pair via `bluetoothctl` (`scan on`, `pair <MAC>`, `connect
  <MAC>`), then bridge BLE-MIDI to ALSA with
  [`bluez-alsa`/`midi`](https://github.com/bluez/bluez) or `btmidi` so it
  appears as an ALSA MIDI port. (Linux BLE-MIDI exposure varies by distro;
  this is OS-level setup, not OpenRig.)

Verify the OS sees it (macOS example): the device appears in *Audio MIDI
Setup*; on Linux `aconnect -i` lists the port.

### 3. Find the chain/block ids to target

`chain`/`block` in the map are the project's **string ids**
(`chain:<uuid>`, `chain:<uuid>:block:<uuid>`), not positions. Get them from
the running rig — easiest first:

- **`openrig://ids` MCP resource** (with `--mcp` on): a flat, copy-paste-ready
  listing of every chain and block with its full id, instrument/kind, and
  enabled state. This is the intended path — no YAML grepping.
- `openrig://project` resource — the whole project YAML, if you want context.
- Fallback: open the project file (`project.yaml` / the `.openrig`) and copy
  the `id:` of the chain/block by hand.

### 4. Write `midi-map.yaml`

Put it at the default path (see table above) or pass `--midi=PATH`. Example
for a Chocolate sending **Program Change 0..3 on ch 1** (factory default),
plus a CC expression input if your unit has one:

```yaml
input: Chocolate          # case-insensitive substring of the MIDI port name

bindings:
  # Switch 1 → load the clean preset
  - source: { kind: program_change, program: 0 }
    command: LoadProject
    args: { path: presets/clean.yaml }

  # Switch 2 → load the crunch preset
  - source: { kind: program_change, program: 1 }
    command: LoadProject
    args: { path: presets/lead.yaml }

  # Switch 3 → toggle the delay block on/off
  - source: { kind: program_change, program: 2 }
    command: ToggleBlockEnabled
    args: { chain: "<chain-id>", block: "<delay-block-id>" }

  # Switch 4 → save the project
  - source: { kind: program_change, program: 3 }
    command: SaveProject

  # Expression pedal on CC 11, ch 1 → ride amp gain 0..100
  - source: { kind: cc, channel: 1, controller: 11 }
    command: SetBlockParameterNumber
    args: { chain: "<chain-id>", block: "<amp-block-id>", path: gain }
    scale: { min: 0.0, max: 100.0 }
```

If a switch is set to **Note** instead, use
`source: { kind: note_on, channel: <ch>, note: <n> }`; for **CC**,
`source: { kind: cc, channel: <ch>, controller: <n> }`.

### 5. Run and verify

```
openrig --midi              # GUI + MIDI adapter on the default map
openrig --midi=~/maps/chocolate.yaml
```

On start the log prints the matched input port
(`adapter-midi: listening on 'Chocolate ...'`). Press a switch — the bound
action happens on the live rig, and the GUI updates in real time (footswitch,
GUI and MCP all share one `ProjectSession`).

### Troubleshooting

- **"no MIDI input port matched ..."** — the `input:` substring didn't match.
  Remove `input:` to use the system default, or set it to a substring of the
  exact port name the log lists.
- **Adapter refuses to start, logs `binding #N (...)`** — that binding's
  command name or args don't match the `Command` schema. Fix the YAML; the
  daemon never starts with a half-valid map (no silently dropped bindings).
- **Nothing happens on a press** — the pedal is sending a different
  type/number/channel than the binding. Re-check it in the MVAVE app monitor
  and align `kind`/number/`channel`.
- **Pedal not in the OS MIDI list** — it isn't paired/connected at the OS
  level yet; redo step 2 (this is never an OpenRig step).

## Scope (v1)

In: USB-MIDI + BLE-MIDI input, the YAML mapping above, Note/CC/Program Change,
linear scale. Out (follow-ups): a mapping editor UI, per-project maps, MIDI
**output** / LED feedback to the controller, log scaling, hot-reload, OSC
(behind a future Cargo feature).

## Design

`docs/superpowers/specs/2026-05-18-22-midi-osc-adapter-design.md` — realizes
Phase 3 of `docs/superpowers/specs/2026-04-23-command-dispatch-architecture-design.md`.
