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

## Want different mappings?

The map file is plain text: one block per binding, `source` (the MIDI
message) → `command` (the OpenRig action) → `args`. Copy a block, change
the `note`/`controller`/`channel`, done. Any OpenRig action can be
bound — the standard map covers the live ones; the rest (block
parameters from a knob, save project, etc.) follow the same shape. See
the comments inside `examples/midi-map.default.yaml` for the full list
and exact syntax.

---

## Scope & guarantees

- One opt-in input that attaches to the running OpenRig through the
  command bus — same path the GUI and MCP use. **No audio-thread code is
  touched**; real-time invariants hold by construction.
- Multiple controllers on different MIDI channels can be used at once
  (full multi-device support is tracked separately).
