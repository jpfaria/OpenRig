# Control OpenRig with a MIDI controller (#22)

You can drive OpenRig live with any MIDI controller — a footswitch like
the **M-Vave Chocolate**, a pedalboard, a knob/fader box, an iPad app.

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

## M-Vave Chocolate — quick setup

The Chocolate (and Chocolate **Plus**) is a 4-switch BLE-MIDI footswitch.

1. Pair it with your computer over Bluetooth so it shows up as a MIDI
   input (macOS: *Audio MIDI Setup → Bluetooth*; Windows: *Settings →
   Bluetooth*; Linux: pair, it appears as an ALSA MIDI port).
2. In **CubeSuite**, set the 4 footswitches. With only 4 switches, pick
   the 4 actions you want from the table above — e.g.:
   - Switch A → Note **60** (previous preset)
   - Switch B → Note **61** (next preset)
   - Switch C → Note **66** (toggle left block)
   - Switch D → Note **67** (toggle right block)
   All on channel **1**, type **Note**.
3. On the **Plus**, each switch's channel is set per message, so a
   second pedal (or a bank) can use the same notes on **channel 2/3/…**
   to reach a different set of actions.
4. `openrig --midi` — press a switch, the rig responds and the screen
   moves in real time.

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

### In the standard map (the table at the top)

| Action | `command` | `args` |
|---|---|---|
| Previous / next preset | `ApplyRigNav` | `{ chain, kind: { StepPreset: -1 } }` / `{ StepPreset: 1 }` |
| Previous / next scene | `ApplyRigNav` | `{ chain, kind: { StepScene: -1 } }` / `{ StepScene: 1 }` |
| Move block selection back / forward | `SelectChainBlock` | `{ chain, delta: -2 }` / `{ delta: 2 }` |
| Toggle left / right block of pair | `ToggleSelectedBlock` | `{ chain, side: Left }` / `{ side: Right }` |
| Chain volume (knob) | `SetChainVolume` | `{ chain }` + `scale: { min: 0, max: 200 }` |

### Other actions you can map (one free Note/CC each)

| Action | `command` | `args` |
|---|---|---|
| Jump to a fixed preset position `n` | `ApplyRigNav` | `{ chain, kind: { Preset: n } }` |
| Jump to a fixed scene `n` | `ApplyRigNav` | `{ chain, kind: { Scene: n } }` |
| Fold pending edits back into the rig | `CaptureRigEdits` | *(none)* |
| Toggle a whole chain on/off | `ToggleChainEnabled` | `{ chain }` |
| Toggle one fixed block on/off | `ToggleBlockEnabled` | `{ chain, block }` |
| Set chain volume to a fixed % (button) | `SetChainVolume` | `{ chain, value: 80 }` |
| Set a numeric param (knob) | `SetBlockParameterNumber` | `{ chain, block, path }` + `scale` |
| Set an on/off param | `SetBlockParameterBool` | `{ chain, block, path, value: true }` |
| Set a text param | `SetBlockParameterText` | `{ chain, block, path, value: "x" }` |
| Pick a list option | `SelectBlockParameterOption` | `{ chain, block, path, value, index }` |
| Point a param at a file | `PickBlockParameterFile` | `{ chain, block, path, file }` |
| Swap a block's model | `ReplaceBlockModel` | `{ chain, block, model_id }` |
| Move a chain up / down | `MoveChainUp` / `MoveChainDown` | `{ chain }` |
| Remove a chain | `RemoveChain` | `{ chain }` |
| Rename the project | `UpdateProjectName` | `{ name: "My Rig" }` |
| Save the project | `SaveProject` | *(none)* |

### Editor-grade (mappable, but take whole objects — not hand-written)

These exist and are bindable, but their `args` is a full structured
object the editor produces, so you don't write them by hand in a map:
`AddBlock`, `InsertPrebuiltBlock`, `OverwriteBlock`, `RemoveBlock`,
`MoveBlock`, `SaveInsertBlock`, `AddChain`, `ConfigureChain`,
`SaveChain`, `SaveChainInputEndpoints`, `SaveChainOutputEndpoints`,
`SaveChainIo`, `LoadChainPreset`, `LoadProject`, `CreateProject`,
`SaveAudioSettings`.

That is **all 34 commands**. `chain`/`block` are the string ids on the
Chains screen; for rig chains the id is `rig:<input>`.

---

## Scope & guarantees

- One opt-in input that attaches to the running OpenRig through the
  command bus — same path the GUI and MCP use. **No audio-thread code is
  touched**; real-time invariants hold by construction.
- Multiple controllers on different MIDI channels can be used at once
  (full multi-device support is tracked separately).
