# Spatial (Atmos-like) output

Status: idea / design. Issue #1079.

Goal: OpenRig as the first pedalboard able to play a show in an immersive (Atmos-like) format.

## What changes in OpenRig

1. **Invariant 5 exception.** Today every stream is stereo internally. A stream may now end in an
   N-channel spatial output (7.1.4 = 12 channels). Everything before that output stays mono/stereo.
2. **No Dolby renderer live.** The real Dolby Atmos renderer and bitstream are licensed. Live, OpenRig
   renders the positions itself (object panner, VBAP-style) straight to device outputs (Quantum HD 8 +
   ADA). Same sound, no Dolby badge. ADM BWF export for recordings is a later option.
3. **Spatial output block.** The last block of a chain takes the stereo signal and writes the N outputs.
   Isolation is unchanged: each stream spreads only its own signal; summing across streams stays in the
   backend (cpal/JACK).

## Effects

- Amp, drive, comp, EQ, cab: unchanged. They run before positioning and never know about space.
- Spatial-only effects (new):
  - Positioner: static object or LFO motion (guitar circling the room).
  - Multichannel reverb (7.1.4 or ambisonic IR).
  - Delay whose repeats hop between speakers.
  - 3D rotary/Leslie.
  - Multichannel chorus/width.

## How the sound is split

Per stream (guitar, bass, synth...), the dry tone is an **object** with a position; effects decide what
reaches the rest of the room:

| Part of the signal | Where it goes |
|---|---|
| Dry amp/cab tone | Object anchored front (L/C/R), or wherever the player stands |
| Reverb tail | Sides, rears and heights (the room around the audience) |
| Delay repeats | Travel speaker to speaker (front → side → rear → top) |
| Modulation / rotary | Moves the object over time |
| Below ~80 Hz (bass, kick) | LFE / sub; guitar normally does not feed the sub |

## Two output modes

1. **Direct render (small room, home studio, own rig):** OpenRig renders to a fixed layout
   (7.1.4, 5.1.4, 5.1) and drives the speakers directly.
2. **Object out (real venue):** a fixed 7.1.4 does not scale to a venue. Immersive live systems (L-ISA,
   d&b Soundscape, Spat Revolution) render from objects. OpenRig sends each object as a dry stem plus its
   position over **ADM-OSC** (open standard), and the venue processor renders to its speakers. This is
   the path to "a show in Atmos" without owning the PA.

## Open questions

- Layout definition: per project (travels with `project.yaml`) or per system? By ADR 0003 the speaker
  layout is per system (it describes the room), the object positions are per project.
- Real-time cost of the panner and the multichannel reverb on the audio thread.
