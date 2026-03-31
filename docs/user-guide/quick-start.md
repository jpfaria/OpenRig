# Quick Start Guide

This guide walks you through setting up OpenRig and building your first signal chain. By the end, you will have a working virtual pedalboard with real-time audio processing.

## Core Concepts

Before diving in, familiarize yourself with the key building blocks of OpenRig:

- **Project** -- A workspace that contains your chains and settings. Projects are saved as YAML files, making them easy to version control and share.

- **Chain** -- A signal path from input to output. Each chain processes audio through a sequence of blocks. Every chain has an instrument type that determines which blocks are available.

- **Block** -- A single audio processor in the chain, such as an amp, effect pedal, or cabinet. Each block has a type and a model.

- **Model** -- A specific implementation of a block type. For example, "Marshall JCM 800 2203" is a model of the Preamp block type.

- **Parameter** -- An adjustable value on a block (gain, bass, treble, mix, and so on). Parameters can be adjusted in real time while audio is playing.

- **Instrument** -- The instrument type assigned to a chain. Available types are `electric_guitar`, `acoustic_guitar`, `bass`, `voice`, `keys`, `drums`, and `generic`. The instrument type filters which blocks are available for the chain.

- **Backend** -- The audio engine that powers a model. OpenRig supports four backends:
  - **Native** -- Built-in Rust DSP processing
  - **NAM** -- Neural Amp Modeler captures
  - **IR** -- Impulse Response convolution
  - **LV2** -- External LV2 plugins

## Step 1: Launch OpenRig

Open the application. You will see the Launcher screen with options to create a new project or open an existing one.

## Step 2: Create a New Project

Click **New Project** and enter a project name. This creates a workspace with a default chain ready for you to customize.

## Step 3: Configure Audio Devices

Navigate to **Settings** and configure the following:

1. **Audio input device** -- Select your guitar interface or microphone.
2. **Audio output device** -- Select your headphones or studio monitors.
3. **Sample rate** -- 48 kHz is recommended for most setups.
4. **Buffer size** -- 256 samples provides a good balance between latency and stability. Lower values reduce latency but may cause audio glitches on slower hardware.

## Step 4: Build Your Chain

Your chain starts with an **Input** block and ends with an **Output** block. To add processing blocks between them:

1. Click the **add button** between two existing blocks.
2. Choose a block type (e.g., Preamp, Cab, Delay, Reverb).
3. Select a model (e.g., "Brit Crunch", "Brit 4x12").
4. The new block appears in the chain and begins processing audio immediately.

Here is an example signal chain for a classic rock tone:

```
Input -> Brit Crunch (Preamp) -> Brit 4x12 (Cab) -> Analog Warm (Delay) -> Plate Foundation (Reverb) -> Output
```

## Step 5: Adjust Parameters

Click on any block to open the **Block Editor**. Use the knobs and sliders to shape your sound. Common parameters by block type:

| Block Type | Parameters                                    |
|------------|-----------------------------------------------|
| Preamp     | gain, bass, middle, treble, presence, master  |
| Cab        | low cut, high cut, mic position, room mix     |
| Delay      | time (ms), feedback, mix                      |
| Reverb     | room size, damping, mix                       |

All changes are applied in real time. Play your instrument while adjusting parameters to hear the effect immediately.

## Step 6: Save Your Work

Your project auto-saves as you work. You can also export presets to share specific chain configurations with others.

**Tip:** Pitch blocks (autotune) are also available for real-time vocal and instrument pitch correction. Add a Chromatic Autotune or Scale Autotune block to any chain that uses a monophonic source.

## What's Next?

- [Blocks Reference](blocks-reference.md) -- Explore all 170+ available models across every block type.
- [Presets](presets.md) -- Learn about creating and sharing presets.
- [Installation Guide](installation.md) -- Detailed build and setup instructions.
