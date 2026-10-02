# Quick Start

From a fresh install to playing through your first chain.

## 1. Install

See [Installing OpenRig](installation.md).

## 2. Pick your audio interface

On the first launch OpenRig asks for the input of your audio interface, then its output, with the sample rate and buffer size. 48 kHz and a buffer of 128–256 samples is a good start: a smaller buffer means less latency, too small causes clicks. You can change this later in **Settings → Audio interface**.

This also creates the `default` **I/O binding**: a named set of input and output channels that chains play through. **Settings → I/O bindings** lets you add more, for example one per guitar or one per pair of outputs.

## 3. Create a project

On the launcher, create a new project and give it a name. A project holds your chains with their presets and scenes, in one `.yaml` file.

## 4. Add a chain

Create a **New chain**: give it a name, pick the instrument (it decides which blocks are offered) and the I/O binding it plays through. Turn the chain on with its power switch and you hear your instrument dry.

## 5. Add blocks

Click **+** on the chain where the block should go and pick a block type. The block editor opens: choose the model, turn the knobs while you play, and confirm. Click a block later to edit it; drag it to move it.

A typical guitar chain: gain pedal → preamp or amp → cab → delay → reverb. Every block type and model is listed in the [blocks catalog](../blocks-catalog.md).

## 6. Save

Save the project from the top bar. Unsaved changes are flagged there until you do.

## Words used in OpenRig

- **Chain** — one signal path: an input, a list of blocks, an output. Each chain runs on its own, so several players or instruments can share one interface.
- **Block** — one processor in the chain (amp, cab, delay, …). Its **model** is the specific unit it emulates, and its **parameters** are the knobs.
- **Preset** and **scene** — a preset is a chain's set of blocks; a scene is a variation of its knob values. See [Presets](presets.md).
- **I/O binding** — a named set of interface channels a chain plays through.
- **Backend** — what runs a model: built-in DSP, NAM captures, impulse responses (IR), LV2 or VST3 plugins.

## Next

- [Presets and scenes](presets.md)
- [Blocks catalog](../blocks-catalog.md)
- [Screens](../screens.md)
- [MIDI controllers](../midi.md)
