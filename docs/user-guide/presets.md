# Presets

## What is a Preset?

A preset is a saved signal chain configuration. It captures the complete signal path including all blocks, their order, selected models, parameter values, and enabled/disabled state. Presets are stored as YAML files, making them human-readable and easy to edit, version, and share.

Each preset represents a complete guitar tone -- from input to output -- ready to be loaded and played.

## YAML Format

Presets follow a simple, flat YAML structure. Below is an annotated example:

```yaml
# --- Preset Identity ---
id: default                      # Unique identifier used internally
name: guitar 1                   # Display name shown in the UI

# --- Signal Chain ---
# Blocks are processed in order, top to bottom.
# Each block represents one stage in the signal path.
blocks:

  # GAIN STAGE — Overdrive / distortion pedal
  - type: gain                   # Block type (gain, dynamics, preamp, cab, delay, reverb, modulation, etc.)
    enabled: true                # Whether this block is active in the signal path
    model: blues_overdrive_bd_2  # The specific model loaded into this block
    params:                      # Model-specific parameters
      gain_percent: 75.0         # Drive amount (0-100)

  # DYNAMICS — Compressor
  - type: dynamics
    enabled: true
    model: compressor_studio_clean
    params:
      attack_ms: 10.0            # Attack time in milliseconds
      makeup_gain: 50.0          # Output gain compensation
      mix: 100.0                 # Dry/wet blend (0-100)
      ratio: 16.0                # Compression ratio
      release_ms: 80.0           # Release time in milliseconds
      threshold: 70.0            # Threshold level

  # PREAMP — Amplifier model (the core of the tone)
  - type: preamp
    enabled: true
    model: brit_crunch
    params:
      bass: 50.0                 # Low-frequency EQ
      bright: false              # Bright switch (boolean toggle)
      depth: 48.0                # Low-end resonance depth
      gain: 56.0                 # Preamp gain / drive
      input: 50.0                # Input level
      master: 62.0               # Master volume
      middle: 50.0               # Mid-frequency EQ
      output: 50.0               # Output level
      presence: 58.0             # High-frequency presence
      sag: 24.0                  # Power amp sag (feel/response)
      treble: 50.0               # High-frequency EQ

  # CABINET — Speaker cabinet with impulse response
  - type: cab
    enabled: true
    model: brit_4x12
    params:
      air: 26.0                  # Room air / ambience
      high_cut_hz: 7200.0        # High-frequency rolloff in Hz
      low_cut_hz: 100.0          # Low-frequency rolloff in Hz
      mic_distance: 24.0         # Microphone distance from speaker
      mic_position: 50.0         # Microphone position (center to edge)
      output: 50.0               # Output level
      resonance: 55.0            # Cabinet resonance
      room_mix: 12.0             # Room reflection mix

  # DELAY — Time-based echo effect
  - type: delay
    enabled: false               # Disabled — present in the chain but bypassed
    model: digital_clean
    params:
      feedback: 35.0             # Number of repeats (0-100)
      mix: 30.0                  # Dry/wet blend
      time_ms: 380.0             # Delay time in milliseconds

  # REVERB — Spatial ambience
  - type: reverb
    enabled: true
    model: plate_foundation
    params:
      damping: 35.0              # High-frequency absorption
      mix: 25.0                  # Dry/wet blend
      room_size: 45.0            # Simulated room size

  # MODULATION — Tremolo, chorus, phaser, etc.
  - type: modulation
    enabled: false               # Disabled — bypassed
    model: tremolo_sine
    params:
      depth: 50.0                # Effect intensity
      rate_hz: 4.0               # Modulation speed in Hz
```

**Key points:**

- **Block order matters.** The signal flows through blocks from top to bottom, just like a physical pedalboard.
- **`enabled: false`** keeps the block in the chain but bypasses it. This lets you toggle effects on and off without losing settings.
- **Parameter values** are model-specific. Different models within the same block type may expose different parameters.
- **All numeric values are floats.** Boolean parameters (like `bright`) are the exception.

## Example Chains

The following examples illustrate common signal chain configurations. Use them as starting points and adjust to taste.

### 1. Clean

A sparkling clean tone with natural headroom and subtle ambience.

```
Input -> American Clean (Preamp) -> American 2x12 (Cab) -> Plate Foundation (Reverb) -> Output
```

**Key settings:**
- Preamp gain low (~20-30%) for a pristine clean signal
- Treble boosted for sparkle and clarity
- Reverb mix around 25% for subtle room ambience
- No gain stage or compression needed

This chain works well for jazz, funk, fingerpicking, and any style that demands a transparent, uncolored tone.

### 2. Classic Rock Crunch

A warm British crunch with enough gain to break up on hard strumming while cleaning up with your guitar's volume knob.

```
Input -> Brit Crunch (Preamp) -> Brit 4x12 (Cab) -> Analog Warm (Delay) -> Plate Foundation (Reverb) -> Output
```

**Key settings:**
- Preamp gain around 56-60% for responsive crunch
- Presence at 58% for cut and definition
- Delay at 380ms with low mix for depth without clutter
- Reverb mix around 20% to add space without washing out the tone

Ideal for classic rock, blues-rock, and rhythm work where touch sensitivity matters.

### 3. High Gain

A modern high-gain tone for metal and hard rock, tight and aggressive with controlled noise.

```
Input -> Ibanez TS9 (Gain) -> Modern High Gain (Preamp) -> Brit 4x12 (Cab) -> Noise Gate (Dynamics) -> Output
```

**Key settings:**
- TS9 drive set low -- used as a boost to tighten the low end before the preamp, not for its own distortion
- Preamp gain set high for saturated distortion
- Noise gate placed after the cab to cut silence noise and keep palm mutes tight

The TS9-into-high-gain technique is a staple of modern metal tone. The overdrive pedal pushes the preamp harder while filtering out low-end mud.

### 4. Acoustic Simulation

Simulates acoustic body resonance for electric guitars or enhances acoustic pickups.

```
Input -> Body (Cab, e.g. Taylor or Martin model) -> Three Band EQ (Filter) -> Plate Foundation (Reverb) -> Output
```

**Key settings:**
- Body model selected for the desired wood tone and resonance character
- EQ adjusted to taste -- cut harsh frequencies, boost warmth
- Reverb for natural ambience, simulating an acoustic space

This chain is useful for unplugged tones through an electric guitar or for shaping the sound of an acoustic-electric instrument.

### 5. Vocal with Autotune

A pitch-corrected vocal chain with dynamics control and spatial effects.

```
Input -> Scale Autotune (Pitch) -> Studio Clean Compressor (Dynamics) -> Three Band EQ (Filter) -> Plate Foundation (Reverb) -> Output
```

**Key settings:**
- Scale Autotune speed at 15ms for natural correction without robotic artifacts
- Key set to C, scale set to Major -- adjust to match the song
- Sensitivity at 40% to avoid correcting silence or breath noise
- Compressor for even vocal dynamics, taming peaks while preserving expression
- Reverb mix around 20% for presence without washing out the vocal

This chain works well for live vocal processing and recording monitoring. For a more pronounced autotune effect, lower the speed toward 0ms.

## Sharing Presets

Presets are plain YAML files, which makes sharing straightforward:

- **Copy and share freely.** Preset files are self-contained and portable. Send them via email, messaging, or any file transfer method.
- **Place preset files in your project directory.** OpenRig will detect and list them automatically.
- **Community sharing.** Presets can be shared through GitHub repositories, forums, or any file hosting platform. No special packaging or conversion is required.
- **Version control friendly.** Because presets are plain text, they work naturally with Git and other version control systems. Track changes, review diffs, and collaborate on tone design just like code.
