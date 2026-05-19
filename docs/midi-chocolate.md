# M-Vave Chocolate / Chocolate Plus — setup

Device-specific guide. For the generic concept, the standard map and the
full action list, read [`docs/midi.md`](midi.md) first — this page only
covers the Chocolate's own quirks and its editor app.

The Chocolate is a 4-footswitch **BLE-MIDI** controller. The **Plus**
adds per-message MIDI channel and more banks; both are configured with
the **CubeSuite** app (a.k.a. *FootCtrlPlus*), free for
Windows/macOS/iOS/Android.

## 1. Pair it

It connects over Bluetooth LE. Put it in pairing mode (hold the switch
combo from its manual until the LED blinks), then:

- **macOS** — *Audio MIDI Setup → Window → Show MIDI Studio → Bluetooth*
  → **Connect** next to "Chocolate".
- **Windows** — *Settings → Bluetooth & devices → Add device* → pick
  "Chocolate".
- **Linux** — pair via your BT manager; it shows up as an ALSA MIDI
  port.

The same Bluetooth radio can only talk to one host at a time: to edit in
CubeSuite the app must hold the pedal, so edit with OpenRig closed, then
reopen OpenRig.

## 2. Set each footswitch in CubeSuite

CubeSuite has no per-switch "channel" box on the main screen — the
message lives **inside a bank entry**. Per footswitch:

1. Tick the footswitch under **Foot Switch [A] / [B] / [C] / [D]**.
2. Pick **Advanced custom mode** (Mode selection) if not already.
3. In the bank box (e.g. "A Bank"), **double-click the entry line**
   (it reads like `[1] 1 PC 0 0`). The **Edit MIDI Code** dialog opens.
4. In that dialog set:
   - **Midi Type → Note**
   - **Channel → 1**
   - **Data1 → the note number** for the action you want
   - **OK**
5. Repeat for the other switches. Then press **Export** to write the
   config to the pedal.

The bank line format is `[index] channel TYPE data1 data2`.

## 3. Which notes — match the standard map

OpenRig's standard map (in `docs/midi.md`) listens on **channel 1**,
notes **60–67**. With only 4 switches, pick 4 actions. A common layout:

| Footswitch | Note (Data1) | Action |
|---|---|---|
| A | 60 | previous preset |
| B | 61 | next preset |
| C | 66 | toggle left block of the pair |
| D | 67 | toggle right block of the pair |

All **Note**, **channel 1**. Want scenes or block-nav instead? Just pick
the matching note from the table in `docs/midi.md`.

## 4. Many pedals / many banks → one channel each

OpenRig opens **every** matching MIDI port at once, so 4 — or 10 —
Chocolates work simultaneously. They send the **same notes**, so to
tell them apart you give **each pedal (or each bank) its own MIDI
channel**, and add the matching map lines on that channel.

On the **Chocolate Plus** the **Channel** field in *Edit MIDI Code* is
**per message**, so:

- **Several pedals:** pedal 1 → all messages on **channel 1**, pedal 2
  → **channel 2**, pedal 3 → **channel 3**, … (notes stay 60–67).
- **One pedal, several banks:** bank A on **channel 1**, bank B on
  **channel 2**, etc. — stepping a bank on the pedal changes which
  channel its switches send on.

Then in the map, the standard lines (channel 1) cover the first
pedal/bank; for each extra channel **copy the lines and change only
`channel`**. Example — second pedal/bank does scenes on channel 2:

```yaml
  - source: { kind: note_on, channel: 2, note: 60 }   # pedal/bank 2, sw A
    command: ApplyRigNav
    args: { chain: "rig:guitar", kind: { StepScene: -1 } }
  - source: { kind: note_on, channel: 2, note: 61 }   # pedal/bank 2, sw B
    command: ApplyRigNav
    args: { chain: "rig:guitar", kind: { StepScene: 1 } }
```

So: same notes everywhere, the **channel** is what routes a given
pedal/bank to a given set of actions. (The plain Chocolate is fixed to
one channel — only the Plus does per-message channel; with plain ones
you instead give each pedal *different note numbers*.)

## 5. Run

Install the standard map and start OpenRig with MIDI on (see
[`docs/midi.md`](midi.md#turn-it-on)):

```
openrig --midi
```

Press a switch — the rig changes and the screen moves in real time.
