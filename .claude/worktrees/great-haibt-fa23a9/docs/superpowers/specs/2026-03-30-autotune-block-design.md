# Autotune Block — Design Spec

**Date:** 2026-03-30
**Status:** Approved

## Goal

Add real-time pitch correction (autotune) to OpenRig as two native models in the `block-pitch` crate. One corrects to the nearest chromatic note, the other corrects to notes within a user-selected musical scale. Both use PSOLA for high-quality pitch shifting and AMDF for pitch detection.

## Models

| Model ID | Display Name | Description |
|----------|-------------|-------------|
| `native_autotune_chromatic` | Chromatic Autotune | Corrects pitch to nearest chromatic note |
| `native_autotune_scale` | Scale Autotune | Corrects pitch to nearest note in selected key/scale |

Both models:
- Brand: `native`
- Backend: `Native`
- Supported instruments: `ALL_INSTRUMENTS` (voice, guitar, bass, keys — any monophonic source)
- Audio mode: `MonoToStereo` — downmix stereo input to mono, process, duplicate to stereo output

## Parameters

### Chromatic Autotune

| Parameter | Display | Min | Max | Default | Step | Unit | Description |
|-----------|---------|-----|-----|---------|------|------|-------------|
| speed | Speed | 0 | 100 | 20 | 1 | ms | Correction speed. 0 = instant/robotic, 100 = natural/smooth |
| mix | Mix | 0 | 100 | 100 | 1 | % | Dry/wet blend |
| detune | Detune | -50 | 50 | 0 | 1 | cents | Fine offset from target note |
| sensitivity | Sensitivity | 0 | 100 | 50 | 1 | % | Minimum signal level to activate correction. Below threshold = pass-through |

### Scale Autotune

Same as Chromatic, plus:

| Parameter | Display | Min | Max | Default | Step | Unit | Description |
|-----------|---------|-----|-----|---------|------|------|-------------|
| key | Key | 0 | 11 | 0 | 1 | enum | Root note: 0=C, 1=C#, 2=D, 3=D#, 4=E, 5=F, 6=F#, 7=G, 8=G#, 9=A, 10=A#, 11=B |
| scale | Scale | 0 | 7 | 0 | 1 | enum | Scale type (see table below) |

### Scales

| Value | Name | Intervals (semitones from root) |
|-------|------|-------------------------------|
| 0 | Major | 0, 2, 4, 5, 7, 9, 11 |
| 1 | Natural Minor | 0, 2, 3, 5, 7, 8, 10 |
| 2 | Pentatonic Major | 0, 2, 4, 7, 9 |
| 3 | Pentatonic Minor | 0, 3, 5, 7, 10 |
| 4 | Harmonic Minor | 0, 2, 3, 5, 7, 8, 11 |
| 5 | Melodic Minor | 0, 2, 3, 5, 7, 9, 11 |
| 6 | Blues | 0, 3, 5, 6, 7, 10 |
| 7 | Dorian | 0, 2, 3, 5, 7, 9, 10 |

## Architecture

### File Structure

```
crates/block-pitch/src/
  lib.rs                          ← PitchModelDefinition registry, build.rs pattern
  core_pitch_detect.rs            ← AMDF pitch detection (adapted from tuner)
  core_psola.rs                   ← PSOLA pitch shifting engine
  core_scales.rs                  ← Scale definitions and target note selection
  native_autotune_chromatic.rs    ← MODEL_DEFINITION + params + builder
  native_autotune_scale.rs        ← MODEL_DEFINITION + params + builder
```

### Registry

`block-pitch` currently has no registry pattern. Must add:
- `PitchModelDefinition` struct (mirror `ModModelDefinition` from block-mod)
- `build.rs` that scans for `MODEL_DEFINITION` constants
- `generated_registry.rs` auto-generated array

The existing `octave_simple` schema in `lib.rs` will be removed or refactored into its own model file once it has an actual implementation.

### DSP Pipeline

Per buffer (called from audio thread):

```
Stereo Input [L, R]
    │
    ▼
Downmix to Mono: (L + R) / 2
    │
    ▼
Pitch Detection (AMDF)
    → detected_frequency_hz
    │
    ▼
Target Note Selection
    → Chromatic: nearest semitone
    → Scale: nearest semitone in key+scale
    → Apply detune offset (cents)
    │
    ▼
Shift Calculation
    → shift_semitones = target - detected
    → Smooth with speed parameter (1st order low-pass)
    │
    ▼
PSOLA Pitch Shifting
    → Applies shift_semitones to audio
    │
    ▼
Sensitivity Gate
    → If input RMS < threshold: output = dry input
    │
    ▼
Mix: output = dry * (1 - mix) + wet * mix
    │
    ▼
Duplicate to Stereo [out, out]
```

### AMDF Pitch Detection (core_pitch_detect.rs)

Adapted from the tuner's AMDF algorithm in `block-util/src/native_tuner_chromatic.rs`, but modified for audio-thread use:

- Runs every buffer (not lazily like the tuner)
- Accumulates samples in a circular buffer (2048 samples minimum for low frequencies)
- Tests lags corresponding to 65 Hz (C2) through 1000 Hz (B5)
- Returns `Option<f32>` — `None` when signal too weak or no clear pitch
- Optimized for real-time: reuse buffer, avoid allocations

### PSOLA Engine (core_psola.rs)

Pitch-Synchronous Overlap-Add:

1. **Pitch marking** — Use detected period (1/frequency) to identify pitch-synchronous points in the input signal
2. **Grain extraction** — Extract overlapping grains (2x pitch period length) centered on pitch marks, windowed with Hanning
3. **Grain repositioning** — Place grains at new intervals determined by target pitch:
   - Target pitch higher → grains closer together (shorter period)
   - Target pitch lower → grains farther apart (longer period)
4. **Overlap-add** — Sum repositioned grains with crossfade

Key implementation details:
- Circular input buffer: 4096 samples (enough for ~60ms at 48kHz, covers lowest pitch period)
- Grain size: 2x detected pitch period (adaptive)
- Overlap: 50% (standard for PSOLA)
- Window: Hanning
- Output buffer: same size as input buffer
- When no pitch detected: pass-through (no correction)

### Speed Parameter (Pitch Smoothing)

The `speed` parameter controls how fast the correction converges to the target note:

```rust
// 1st order exponential smoothing
let alpha = if speed_ms <= 0.0 {
    1.0  // instant correction
} else {
    1.0 - (-1.0 / (speed_ms * 0.001 * sample_rate)).exp()
};
current_shift = current_shift + alpha * (target_shift - current_shift);
```

- speed=0ms → alpha=1.0 → instant snap (T-Pain effect)
- speed=20ms → fast but smooth (default, studio vocal correction)
- speed=100ms → slow glide (natural, almost imperceptible)

### Target Note Selection (core_scales.rs)

**Chromatic:** Quantize detected frequency to nearest MIDI semitone.

```rust
let midi_note = 12.0 * (freq / 440.0).log2() + 69.0;
let target_midi = midi_note.round();
let target_freq = 440.0 * 2f32.powf((target_midi - 69.0) / 12.0);
```

**Scale-locked:** Quantize to nearest note that belongs to the selected scale in the selected key.

```rust
let note_in_octave = (midi_note.round() as i32).rem_euclid(12);
// Find nearest note in scale intervals, considering key offset
// If note_in_octave is not in scale, snap to closest scale degree
```

**Detune:** After finding target note, apply cents offset:
```rust
let detuned_freq = target_freq * 2f32.powf(detune_cents / 1200.0);
```

### Sensitivity Gate

RMS-based gate to avoid correcting silence/noise:

```rust
let rms = (buffer.iter().map(|s| s * s).sum::<f32>() / buffer.len() as f32).sqrt();
let threshold_linear = sensitivity / 100.0 * 0.1; // 0.0 to 0.1 range
if rms < threshold_linear {
    // Pass-through dry signal, no correction
}
```

- sensitivity=0% → always corrects (even quiet signals)
- sensitivity=50% → reasonable default, ignores noise floor
- sensitivity=100% → only corrects loud signals

## Stereo Handling

The processor implements `StereoProcessor`:

```rust
impl StereoProcessor for AutotuneProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mono = (input[0] + input[1]) * 0.5;
        let corrected = self.process_mono_sample(mono);
        let mixed = mono * (1.0 - self.mix) + corrected * self.mix;
        [mixed, mixed]
    }
}
```

Note: `process_block()` should be overridden for efficiency — PSOLA works on buffers, not individual samples. The per-sample `process_frame()` will accumulate into an internal buffer and flush when full.

## Integration

### block.rs in project crate

Add pitch block instantiation (currently missing). Wire `PitchModelDefinition::build()` into the block creation pipeline, same pattern as other block types.

### Cargo.toml

`block-pitch` needs no new external dependencies. The AMDF and PSOLA algorithms are implemented in pure Rust using `std::f32` math.

```toml
[dependencies]
anyhow.workspace = true
block-core = { path = "../block-core" }
```

## Testing

1. **Pitch detection accuracy** — Feed known sine waves (A4=440Hz, C4=261.63Hz), verify detected frequency within ±1Hz
2. **Chromatic correction** — Feed 445Hz (sharp A4), verify output is 440Hz
3. **Scale correction** — Feed note outside scale, verify snaps to nearest scale degree
4. **Speed parameter** — Feed off-pitch signal, verify convergence time matches speed setting
5. **Sensitivity gate** — Feed quiet signal below threshold, verify pass-through
6. **Mix blend** — Verify dry/wet ratio at 0%, 50%, 100%
7. **Stereo I/O** — Verify stereo input is correctly downmixed and output is duplicated

## Out of Scope

- Polyphonic pitch correction (chords)
- Formant preservation (advanced vocal quality)
- MIDI input for target notes
- Visual feedback (pitch display in UI)
- controls.svg panel design
