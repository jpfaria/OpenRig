# Control OpenRig with a MIDI controller (#22)

You can drive OpenRig live with **any** MIDI controller — a footswitch,
a pedalboard, a knob/fader box, an expression pedal, an iPad app. There
are hundreds of them; this guide is controller-agnostic. Device-specific
setup notes live in their own page (see *Device guides* below).

How it works, in one line: **your controller sends standard MIDI
messages; OpenRig reads a map that turns each message into an action.**
OpenRig ships with a **standard map** so you only have to set your
controller to send the messages in the table below — no editing needed
to start.

MIDI itself only has three message kinds you care about here:

- **Note** — a button/footswitch press.
- **Control Change (CC)** — a knob or expression pedal (a value 0–127).
- **Program Change (PC)** — a "select patch N" message.

---

## What to set on your controller (standard map)

Open your controller's editor (for the M-Vave Chocolate that's the
**CubeSuite** app) and set each footswitch / knob to send exactly this.
Everything is on **MIDI channel 1**.

| You want it to… | Message type | Number | MIDI channel |
|---|---|---|---|
| Go to the **previous preset** | Note | 60 | 1 |
| Go to the **next preset** | Note | 61 | 1 |
| Go to the **previous scene** | Note | 62 | 1 |
| Go to the **next scene** | Note | 63 | 1 |
| Move block selection **back** | Note | 64 | 1 |
| Move block selection **forward** | Note | 65 | 1 |
| Toggle the **left** block of the selected pair | Note | 66 | 1 |
| Toggle the **right** block of the selected pair | Note | 67 | 1 |
| **Chain volume** (turn a knob) | Control Change (CC) | 7 | 1 |

That's the whole hardware side. A footswitch sends its Note when you
press it; a knob sends CC 7 continuously as you turn it (0 = silent,
127 = +6 dB).

Notes:

- Preset and scene wrap around — after the last comes the first.
- "Block selection" is a moving pair of two adjacent blocks; a thin
  border shows on screen when you press a selection footswitch and fades
  away on its own a few seconds later.
- The screen and the sound react to a footswitch exactly like a mouse
  click.

---

## Turn it on

1. Copy the standard map into OpenRig's config folder:

   | OS | Copy `examples/midi-map.default.yaml` to |
   |---|---|
   | macOS | `~/Library/Application Support/OpenRig/midi-map.yaml` |
   | Windows | `%APPDATA%\OpenRig\midi-map.yaml` |
   | Linux | `~/.config/OpenRig/midi-map.yaml` |

2. In that file, change the one line `chain: "rig:guitar"` to your rig
   input's name (the input shown on the Chains screen) — once.

3. Start OpenRig with MIDI on:

   ```
   openrig --midi
   ```

   (or `openrig --midi=/path/to/your-map.yaml` to point at a specific
   file). If the map is missing or a line is wrong, OpenRig refuses to
   start and logs exactly why — it never silently ignores a binding.

---

## Generic setup (any controller)

1. Connect the controller so the OS sees it as a MIDI input (USB: plug
   in; Bluetooth/BLE-MIDI: pair it — macOS *Audio MIDI Setup →
   Bluetooth*, Windows *Settings → Bluetooth*, Linux it appears as an
   ALSA/JACK MIDI port).
2. Open **your controller's editor app** (every brand has one) and set
   each control to send the message from the table above — type
   (Note/CC), number, channel 1. If your controller has fewer controls
   than the table, pick the actions you want most.
3. Tip: keep a MIDI monitor open while you press a control to confirm
   the exact message it sends before relying on it.
4. Install the standard map and run `openrig --midi` (next section).

## Device guides

Brand-specific step-by-step (pairing, the editor app, quirks):

- **M-Vave Chocolate / Chocolate Plus** — see
  [`docs/midi-chocolate.md`](midi-chocolate.md).

More devices will get their own page; the generic setup above works for
any of them.

---

## All actions (every command)

The standard map binds the 9 live actions above. But **every** OpenRig
action can be mapped — you just add a line and pick a free Note/CC for
it. A map line is always:

```yaml
- source: { kind: note_on, channel: 1, note: 70 }   # the MIDI message
  command: ToggleChainEnabled                         # the action
  args: { chain: "rig:guitar" }                       # what it acts on
```

`kind` is `note_on` (button), `cc` (knob — add `scale: { min, max }`),
or `program_change`. Below is **the complete list** — `command` is the
exact name, `args` is what goes in the line.

Legend: **★** = already wired in the standard map (top table).
`id` = string id from the Chains screen (`rig:<input>` for rig chains);
`text` = string; `num` = number; `int` = integer; `uint` = ≥0 integer;
`bool` = true/false; `path` = file path; `object` = a full structured
object the editor produces (not hand-written in a map). Every command
below is bindable.

| # | `command` | What it does | `args` |
|---|---|---|---|
| 1 | `SetBlockParameterNumber` | Set a numeric param (great on a knob + `scale`) | `{ chain: id, block: id, path: text, value: num }` |
| 2 | `SetBlockParameterBool` | Set an on/off param | `{ chain: id, block: id, path: text, value: bool }` |
| 3 | `SetBlockParameterText` | Set a text param | `{ chain: id, block: id, path: text, value: text }` |
| 4 | `SelectBlockParameterOption` | Pick a list option | `{ chain: id, block: id, path: text, value: text, index: uint }` |
| 5 | `PickBlockParameterFile` | Point a param at a file | `{ chain: id, block: id, path: text, file: path }` |
| 6 | `ToggleBlockEnabled` | Toggle one fixed block on/off | `{ chain: id, block: id }` |
| 7 | `ReplaceBlockModel` | Swap a block's model | `{ chain: id, block: id, model_id: text }` |
| 8 | `AddBlock` | Add a block | `{ chain: id, kind: text, model_id: text, position: uint }` |
| 9 | `InsertPrebuiltBlock` | Insert a pre-built block | `{ chain: id, block: object, position: uint }` |
| 10 | `OverwriteBlock` | Replace a block | `{ chain: id, block: id, replacement: object }` |
| 11 | `RemoveBlock` | Remove a block | `{ chain: id, block: id }` |
| 12 | `MoveBlock` | Move a block to a position | `{ chain: id, block: id, new_position: uint }` |
| 13 | `SaveInsertBlock` | Save a block's insert send/return | `{ chain: id, block: id, send: object, return_: object }` |
| 14 | `AddChain` | Add a chain | `{ chain: object }` |
| 15 | `ConfigureChain` | Reconfigure a chain | `{ chain: object }` |
| 16 | `SaveChain` | Save a chain | `{ chain: object }` |
| 17 | `RemoveChain` | Remove a chain | `{ chain: id }` |
| 18 | `MoveChainUp` | Move a chain up in the list | `{ chain: id }` |
| 19 | `MoveChainDown` | Move a chain down in the list | `{ chain: id }` |
| 20 | `ToggleChainEnabled` | Toggle a whole chain on/off | `{ chain: id }` |
| 21 | `SaveChainInputEndpoints` | Replace a chain's inputs | `{ chain: id, input_blocks: [object] }` |
| 22 | `SaveChainOutputEndpoints` | Replace a chain's outputs | `{ chain: id, output_blocks: [object] }` |
| 23 | `SaveChainIo` | Save a chain's input+output | `{ chain: id, input_block: object, output_block: object }` |
| 24 | `LoadChainPreset` | Load a preset into a chain | `{ chain: id, preset_blocks: [object] }` |
| 25 | `SaveProject` | Save the project | *(none)* |
| 26 | `LoadProject` | Load a project | `{ project: object, path: path }` |
| 27 | `CreateProject` | Create a new project | `{ project: object }` |
| 28 ★ | `SetChainVolume` | Set chain volume (% — knob via `scale`, or fixed `value`) | `{ chain: id, value: num }` |
| 29 | `UpdateProjectName` | Rename the project | `{ name: text }` |
| 30 | `SaveAudioSettings` | Save audio device settings | `{ device_settings: [object] }` |
| 31 ★ | `ApplyRigNav` | Preset/scene: step (footswitch) or jump (fixed) | `{ chain: id, kind: <see below> }` |
| 32 ★ | `SelectChainBlock` | Move the block-selection pair cursor (wraps) | `{ chain: id, delta: int }` |
| 33 ★ | `ToggleSelectedBlock` | Toggle one side of the selected pair | `{ chain: id, side: Left or Right }` |
| 34 | `CaptureRigEdits` | Fold pending synthetic-chain edits back into the rig | *(none)* |

`ApplyRigNav`'s `kind` (one of):
`{ Preset: int }` (jump to preset position) ·
`{ Scene: int }` (jump to scene) ·
`{ StepPreset: int }` (relative, e.g. `-1`/`1`, wraps) ·
`{ StepScene: int }` (relative, wraps).

That is **all 34 commands** (enum order). The 9 live actions in the
standard map are: ★31 `ApplyRigNav` StepPreset ±1 and StepScene ±1,
★32 `SelectChainBlock` ±2, ★33 `ToggleSelectedBlock` Left/Right,
★28 `SetChainVolume` on a knob.

---

## Scope & guarantees

- One opt-in input that attaches to the running OpenRig through the
  command bus — same path the GUI and MCP use. **No audio-thread code is
  touched**; real-time invariants hold by construction.
- **Multiple controllers at once** — every input port whose name
  matches `input:` is opened (or all ports if `input:` is omitted), all
  feeding the same command bus. So 4 identical Chocolates, or a
  footswitch + a knob box, work together; tell them apart by MIDI
  channel (set per message on the Chocolate Plus).
