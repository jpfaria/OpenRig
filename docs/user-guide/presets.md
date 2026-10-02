# Presets

A preset is the processing part of a chain: its blocks in order, with their models, parameters and on/off state. It holds no input or output — those belong to the chain — so the same preset plays through whatever interface the chain uses.

## Presets and scenes in a project

Each chain has its own **bank** of presets, saved inside the project file. In the chain's preset selector you can switch presets, **Add preset**, **Remove preset** and **Rename preset**. A footswitch can step through them (see [MIDI](../midi.md)).

Each preset has up to 8 **scenes**. A scene keeps its own knob values and its own on/off state for each block, so one preset can hold a clean, a crunch and a lead version that switch without a gap. Settings that are not numbers (a switch, a choice from a list) are shared by every scene of the preset.

Changes are kept when you save the project.

## The preset library

Besides the bank inside the project, presets can be saved as files to reuse in other projects:

- **Save preset** writes the chain's current blocks to `<name>.yaml` in the presets folder. Saving over an existing name asks first.
- **Load preset into chain** opens a searchable list of the files in the presets folder and replaces the chain's blocks with the one you pick. A preset saved for another instrument type is refused.

The presets folder is set in **Settings → Paths → Presets**. By default it is `presets` inside the app's data folder (macOS `~/Library/Application Support/OpenRig/presets`, Linux `~/.local/share/openrig/presets`).

## Sharing

A preset file is self-contained plain text: send it, put it in Git, or drop it into someone else's presets folder. The other machine needs the same models; a block whose model it does not have is skipped with a warning, and the rest of the preset loads.

## File format

```yaml
version: 1
id: crunch
name: Crunch
volume: 100.0              # output level in percent, 100 = unity
instrument: electric_guitar
blocks:                    # processed top to bottom
- type: preamp
  enabled: true
  model: brit_crunch
  params:
    gain: 56.0
    bass: 50.0
    middle: 50.0
    treble: 50.0
    presence: 58.0
    master: 62.0
- type: cab
  enabled: true
  model: brit_4x12
  params:
    low_cut_hz: 100.0
    high_cut_hz: 7200.0
    room_mix: 12.0
- type: delay
  enabled: false           # kept in the chain, bypassed
  model: digital_clean
  params:
    time_ms: 380.0
    feedback: 35.0
    mix: 30.0
```

- `params` are model-specific; the block editor shows each model's set. A parameter left out takes the model's default.
- `instrument` defaults to `electric_guitar` when missing.
- A split is written as `type: split` with its paths — see [project format](../projects/project-format.md).
