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

## Splitting the effects across speakers

Yes: something has to separate dry from wet, otherwise there is only one stereo signal to place.
The spatial output block works as a router with separate inputs, not as a stereo-to-N upmixer:

- **Dry** (amp/cab) goes to the object panner (front, or the player position).
- **Existing stereo effects** (reverb, delay, modulation already in the catalog) become **sends** with a
  zone: their wet signal is placed on a chosen zone (e.g. reverb wet → rears + heights). No new DSP.
- **Native spatial effects** write straight into the N-channel bus (reverb with 12 outputs, delay that
  pans each repeat), so they never pass through stereo.

A blind upmixer (stereo in, 12 out) is the cheap fallback and sounds worse: dry and wet are already
mixed, so the room cannot be built from them.

## Who decides the positions

Nothing is random. Positions and zones are parameters saved in the preset (they change with the preset or
scene). Motion comes from an LFO synced to the tempo, an expression pedal / MIDI CC (pedal moves the
guitar front → back), or a scripted path. A random mode can exist as an explicit option, never as default.

## Recipes: making the guitar feel spatial

Spatial does not mean wetter. The wet amount stays the player's choice; the layout only changes where
the wet goes. With the reverb on other speakers, it no longer masks the dry tone in the same two
speakers, so the same mix sounds clearer than in stereo (spatial unmasking), and a drier setting works.

1. **Wrap-around ambience:** dry front center; reverb wet on sides + rears + heights, with a few ms more
   pre-delay on the rears and heights. The guitar stays in front, the room surrounds the audience.
2. **Delay that travels:** each repeat lands on the next speaker (front → side → rear → top), synced to
   the tempo. Solo with a dotted-eighth delay circling the audience.
3. **Rotating guitar:** the positioner LFO moves the object around the room in sync with the tempo
   (rotary/Leslie feel, but in the room instead of a cabinet).
4. **Pedal-controlled flight:** the expression pedal moves the guitar from the stage to above the
   audience (front → heights) in a build-up; heel back returns it to the stage.
5. **Wide pad / clean arpeggio:** multichannel chorus with a different modulation phase per speaker, so
   each speaker sounds slightly different and the sound has no single point.
6. **Height for the lead:** rhythm guitar stays at ear level; the lead gets its reverb/delay on the
   heights so it floats above the band.

## Monitoring for the player

The player is on stage, outside the sweet spot: from there the rears and heights are far away or behind
the PA, so the room mix is not what the player hears. The spatial output therefore has a separate
**monitor output**, per stream:

- **In-ear (main case): binaural render.** The same scene rendered with HRTF to 2 channels, with the
  listener placed in the middle of the audience (hear what they hear) or at the player position. On
  in-ears the movement (delay travelling, guitar rotating) is audible in 3D.
- **Wedge / stereo monitor:** stereo fold-down of the scene (no 3D, but nothing is lost).
- **Dry guitar in the monitor stays on the low-latency path.** Binaural rendering and multichannel reverb
  only affect the wet/ambience part; the dry tone the player plays against must not get any extra
  latency (invariant 1).
- Each player's monitor is part of their own stream (isolation): a bass player's binaural mix does not
  depend on the guitar stream.

## Playing for yourself (player in the sweet spot)

The player sits in the middle of their own speaker layout and plays: no monitor needed, the room mix is
what they hear. This is the direct-render mode and the first target (the owner's room already has a
6-speaker test layout).

- **Calibration to the chair:** per-speaker distance, level and delay measured at the listening
  position, so all speakers arrive at the same time and level (the first test had rears 12–19 dB below the
  fronts).
- **Alignment delay only on the ambience:** time alignment delays the nearer speakers to match the
  farthest one. The dry guitar stays on the front speakers without that extra delay, so playing feel is
  not affected; only the wet/ambience is aligned.
- **Mismatched speakers:** with different speakers, a moving sound changes timbre as it passes from one
  to another (a guitar 1x12 has no real highs or lows). Per-speaker correction EQ measured at the chair
  narrows the gap but cannot add range a speaker lacks. Until the layout uses identical speakers, keep
  moving effects (rotation, travelling delay) on matched speakers and use the odd ones for ambience
  (reverb), where timbre mismatch is less audible.
- **Bass management:** in direct render, the spatial output applies an 80 Hz crossover on every speaker
  channel and sums the lows into the sub (LFE) output, so small speakers only play from 80 Hz up and the
  sub is placed once for the whole layout.
- **"Inside the guitar" option:** place the dry object at the listener position (all speakers around)
  instead of in front.

## Two output modes

1. **Direct render (small room, home studio, own rig):** OpenRig renders to a fixed layout
   (7.1.4, 5.1.4, 5.1) and drives the speakers directly.
2. **Object out (real venue):** a fixed 7.1.4 does not scale to a venue. Immersive live systems (L-ISA,
   d&b Soundscape, Spat Revolution) render from objects. OpenRig sends each object as a dry stem plus its
   position over **ADM-OSC** (open standard), and the venue processor renders to its speakers. This is
   the path to "a show in Atmos" without owning the PA.

## Sketch: how it would look in OpenRig

**System (`config.yaml`, describes the room, ADR 0003):** a speaker layout bound to device outputs, with
the calibration measured at the chair.

```yaml
speaker_layouts:
  - id: studio_714
    io: hd8
    speakers:
      - { name: L,   azimuth: -30,  elevation: 0,  endpoint: Out1,  delay_ms: 0.0, gain_db: 0 }
      - { name: R,   azimuth: 30,   elevation: 0,  endpoint: Out2,  delay_ms: 0.0, gain_db: 0 }
      - { name: C,   azimuth: 0,    elevation: 0,  endpoint: Out3,  delay_ms: 0.4, gain_db: -1 }
      - { name: Ls,  azimuth: -100, elevation: 0,  endpoint: Out5 }
      # ... Rs, Lrs, Rrs, Ltf, Rtf, Ltr, Rtr
    sub: { endpoint: Out4, crossover_hz: 80 }
```

**Project (`project.yaml`, travels with the song):** the chain ends in a `spatial_output` block. Positions
and sends are block parameters, so they live in presets and can be MIDI/expression mapped.

```yaml
chains:
  - description: guitar 1
    blocks:
      - { type: input,  io: hd8, endpoint: In1 }
      - { type: preamp, model: marshall_jcm_800_2203 }
      - { type: cab,    model: ... }
      - { type: delay,  model: digital_clean, params: { mix: 100 }, spatial_send: true }
      - { type: reverb, model: plate,         params: { mix: 100 }, spatial_send: true }
      - type: spatial_output
        layout: studio_714            # or adm_osc (venue renderer)
        params:
          dry:    { azimuth: 0, elevation: 0, width: 20 }
          motion: { mode: off }       # off | lfo (tempo-synced) | expression
          sends:
            delay:  { zone: travel, level_db: -6 }   # repeats hop speaker to speaker
            reverb: { zone: rear_and_top, level_db: -9 }
        monitor: { io: hd8, endpoint: Out11, mode: binaural }
```

**Screen:** the `spatial_output` block editor shows the room from above (plus a height slider): the dry
guitar is a dot to drag, each send is a coloured zone, the speakers are drawn from the layout, and the
meters show level per speaker. MCP/gRPC get the same Commands (move object, set send zone, set motion).

## Open questions

- Layout definition: per project (travels with `project.yaml`) or per system? By ADR 0003 the speaker
  layout is per system (it describes the room), the object positions are per project.
- Real-time cost of the panner and the multichannel reverb on the audio thread.
