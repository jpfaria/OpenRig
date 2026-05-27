# M-Vave Chocolate / Chocolate Plus — setup

Device-specific guide. For the generic concept, the standard map and the
full action list, read [`docs/midi.md`](midi.md) first — this page only
covers the Chocolate's own quirks and its editor app.

If you only want the **shipped factory layout** (4 pre-bound banks
covering chain nav, preset/scene, block pair and global toggles), skip
to [`chocolate_plus_program_change_a.md`](../assets/midi-profiles/chocolate_plus_program_change_a.md)
— that file documents what every switch does per bank with the
out-of-the-box profile. The page you are reading is for going beyond
the factory layout (different modes, custom notes, multiple pedals).

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

## 2. How the Chocolate works

Before configuring anything, know the device — it changes which mode
you want, what each footswitch does, and how to flip banks live.

### 2.1 Layout

Four large footswitches across the top, labeled **A B C D**, with a
small LCD between B and C. The back/side strip has the USB-C
power/data port, a USB-A **HOST** port (for the supplied receiver on a
computer that does not speak BLE-MIDI), a 1/4" **PEDAL** jack for an
expression pedal, and a 3-position toggle for *USB / OFF / HOST*.

The labels **E** and **F** are not extra switches — they are *combos*:

- **E** = press **A + B** together
- **F** = press **C + D** together

E and F are how you change banks on the pedal itself, in modes that
support banking.

### 2.2 Modes — pick *Advanced custom 2*

CubeSuite exposes **12 working modes**. Most are for non-MIDI uses
(steering an Android touchscreen, scrubbing YouTube, acting as a USB
keyboard, controlling another M-Vave product) and never talk to
OpenRig. The three you can actually use as a MIDI controller are:

| Mode | What A–D send | Per-message channel | E/F banks |
|---|---|---|---|
| Program change A (PC) | PC 0–127 | one, fixed | yes (up to 16 groups × 4 = 128 PCs) |
| Advanced custom 1 | any (PC/CC/Note/SysEx), 5 sub-behaviours | yes | **no** |
| **Advanced custom 2** | any (PC/CC/Note/SysEx), short-tap + long-press | **yes** | **yes** (up to 16 groups) |

OpenRig's standard map listens for **Note** messages on **channel 1**
(see [`docs/midi.md`](midi.md)), and the multi-bank trick in §5 below
needs per-message channel. Only **Advanced custom 2** gives you both,
so that is the mode the rest of this page assumes.

### 2.3 Banks live, on the pedal

In *Advanced custom 2* the LCD shows the current bank number (1 to N,
N selectable up to 16 in the software). Step it with the combos:

- **A + B together (E)** — previous bank
- **C + D together (F)** — next bank

The change is instant; the next tap on A/B/C/D fires the message
configured for the *new* bank. Pair this with the channel-per-bank
pattern in §5 to multiply 4 switches into 4 × 16 = **64 actions** on
the same pedal without ever running out of notes.

In *Advanced custom 1* (no banking) you get one fixed page of 4
switches with richer sub-behaviours (single tap / two-group toggle /
press-release / long press / short-tap+long-press). That is the trade:
sub-behaviours **or** banking.

### 2.4 Plain Chocolate vs Chocolate Plus

Same 4 footswitches, same E/F combos. The **Plus** is what unlocks the
per-message **channel** field inside *Edit MIDI Code* — the plain
Chocolate sends everything on **one device-wide channel**, so the
"different channel per pedal / per bank" pattern in §5 only works on
the Plus. With a plain Chocolate, give each pedal **different note
numbers** instead.

## 3. Set each footswitch in CubeSuite

> Skip this section if you are happy with the shipped factory
> layout — see
> [`chocolate_plus_program_change_a.md`](../assets/midi-profiles/chocolate_plus_program_change_a.md)
> for what that profile already binds. Come back here when you want
> custom notes, custom banks or a different MIDI channel.

CubeSuite has no per-switch "channel" box on the main screen — the
message lives **inside a bank entry**. Per footswitch:

1. Tick the footswitch under **Foot Switch [A] / [B] / [C] / [D]**.
2. Pick **Advanced custom mode 2** (Mode selection) — *not* Advanced 1,
   which has no banking — if not already.
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

## 4. Which notes — match the standard map

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

## 5. Many pedals / many banks → one channel each

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

## 6. Run

Install the standard map and start OpenRig with MIDI on (see
[`docs/midi.md`](midi.md#turn-it-on)):

```
openrig --midi
```

Press a switch — the rig changes and the screen moves in real time.
