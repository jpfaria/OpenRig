# Additional Reverb Models (Hall, Room, Shimmer, Spring)

**Issue:** #119
**Date:** 2026-04-06
**Status:** Approved

---

## Summary

Expand native reverb models beyond the current Plate Foundation. Add 4 new models: Hall, Room, Spring, and Shimmer. All implemented from scratch in Rust with no external DSP dependencies, following the existing `block-reverb` architecture.

---

## Design Decisions

| Decision | Choice |
|----------|--------|
| Models to implement | All 4: Hall, Room, Spring, Shimmer |
| DSP approach | From scratch in Rust, no external crates |
| Parameter strategy | 3 base params (Decay, Tone, Mix) + 2 specific per model |
| Audio mode | All TrueStereo (`StereoProcessor` trait) |
| Visual identity | Unique personality per model, consistent structure |

---

## Models and Parameters

### Common Base Parameters (all models)

| Parameter | Path | Label | Range | Default | Unit | Description |
|-----------|------|-------|-------|---------|------|-------------|
| Decay | `decay` | Decay | 0-100% | 50% | Percent | Reverb tail length |
| Tone | `tone` | Tone | 0-100% | 50% | Percent | High-frequency damping (0=dark, 100=bright) |
| Mix | `mix` | Mix | 0-100% | 25% | Percent | Dry/wet blend |

### Hall Reverb — `native_hall`

**Display Name:** Hall Reverb
**Character:** Large, enveloping, smooth tails. Ideal for leads and ambient.

| Parameter | Path | Label | Range | Default | Unit | Description |
|-----------|------|-------|-------|---------|------|-------------|
| Pre-Delay | `pre_delay` | Pre-Delay | 0-100 | 30 | Milliseconds | Gap before reverb onset |
| Diffusion | `diffusion` | Diffusion | 0-100% | 70% | Percent | Echo spread density (low=discrete, high=smooth wash) |

### Room Reverb — `native_room`

**Display Name:** Room Reverb
**Character:** Short, natural, prominent early reflections. Ideal for rhythm and clean tones.

| Parameter | Path | Label | Range | Default | Unit | Description |
|-----------|------|-------|-------|---------|------|-------------|
| Size | `size` | Size | 0-100% | 40% | Percent | Room dimensions (small to large) |
| Early Reflections | `early_reflections` | Early Ref | 0-100% | 60% | Percent | Level of discrete wall reflections |

### Spring Reverb — `native_spring`

**Display Name:** Spring Reverb
**Character:** Metallic boing and drip, vintage amp character. Classic Fender sound.

| Parameter | Path | Label | Range | Default | Unit | Description |
|-----------|------|-------|-------|---------|------|-------------|
| Dwell | `dwell` | Dwell | 0-100% | 50% | Percent | Input drive to the spring tank (saturation) |
| Drip | `drip` | Drip | 0-100% | 50% | Percent | Metallic splash intensity on transients |

### Shimmer Reverb — `native_shimmer`

**Display Name:** Shimmer Reverb
**Character:** Ethereal, pitch-shifted layers building in the tail. Organ-like pads.

| Parameter | Path | Label | Range | Default | Unit | Description |
|-----------|------|-------|-------|---------|------|-------------|
| Pitch | `pitch` | Pitch | 0-100% | 50% | Percent | Pitch interval: 0-33%=+5th (+7 semi), 34-66%=+octave (+12), 67-100%=+2 octaves (+24) |
| Shimmer Amount | `shimmer_amount` | Shimmer | 0-100% | 40% | Percent | How much pitch-shifted signal feeds back into the reverb |

---

## DSP Algorithms

### Shared DSP Primitives

All models reuse a common set of building blocks implemented in a shared module (`dsp.rs`):

```rust
// Circular delay line with optional fractional read (cubic interpolation)
struct DelayLine { buffer, write_pos, length }

// First-order allpass filter
struct AllpassFilter { buffer, index, feedback }

// One-pole low-pass (damping)
struct DampingFilter { state, coefficient }

// Comb filter with integrated damping
struct DampedCombFilter { delay_line, feedback, damping_filter }
```

Anti-denormal: add `1e-18` DC offset in all feedback paths.

Sample rate independence: all delay times stored as seconds, converted to samples at init via `(time_sec * sample_rate).round() as usize`.

### Hall — Modified Dattorro (1997) with Early Reflections

The Dattorro plate reverb is the industry standard, used by MVerb, Dragonfly Hall, and Lexicon-style reverbs. For a hall variant, we extend delay lengths and add an early reflection stage.

**Signal flow:**
```
input_L + input_R
    → sum to mono
    → pre-delay line (0-100ms, user-controlled)
    → early reflections (12-tap delay line, Moorer-style)
    → bandwidth LP filter (one-pole, coeff ~0.9995)
    → 4x input diffusion allpass chain
    → tank (figure-of-eight cross-coupled structure)
    → output taps (L/R from multiple points in the tank)
    → dry/wet mix with original stereo input
```

**Input diffusion (4 allpasses in series):**

Diffusion parameter scales the allpass coefficients (0.0-0.75 range). At low diffusion, early echoes are more discrete; at high diffusion, the sound washes smoothly.

| Stage | Delay (at 29761 Hz) | Coefficient |
|-------|---------------------|-------------|
| 1 | 142 samples | +0.75 * diffusion |
| 2 | 107 samples | +0.75 * diffusion |
| 3 | 379 samples | +0.625 * diffusion |
| 4 | 277 samples | +0.625 * diffusion |

All delay lengths scale by `target_sample_rate / 29761.0`. For hall, additionally multiply tank delays by 2.0x to simulate larger space.

**Tank (2 cross-coupled halves):**

Each half contains: modulated decay allpass → pre-damping delay → damping LP → decay multiply → decay allpass → post-damping delay. Output of half 0 feeds into half 1's input and vice versa.

Tank 0:
- Decay allpass 1: delay 672, coeff -0.70 (modulated +-16 samples at ~1Hz via sine LFO)
- Pre-damping delay: 4453 samples
- Damping LP: one-pole, coefficient derived from Tone parameter
- Decay multiply: derived from Decay parameter (0.2 to 0.97 range)
- Decay allpass 2: delay 1800, coeff = clamp(decay + 0.15, 0.25, 0.50)
- Post-damping delay: 3720 samples

Tank 1:
- Decay allpass 1: delay 908, coeff -0.70 (modulated)
- Pre-damping delay: 4217 samples
- Decay allpass 2: delay 2656
- Post-damping delay: 3163 samples

**Output taps:** Multiple taps from both tank halves, some added, some subtracted, producing decorrelated L/R output. Scaling factor 0.6.

**Early reflections (12-tap Moorer-style):**

Tapped delay line with decreasing gains and per-tap LP filtering. Tap times range from 3ms to 80ms. Panned alternately L/R for stereo width.

### Room — Moorer Early Reflections + 8-channel FDN Householder

Two separate stages: prominent early reflections (the "room" character) and a short, dense diffuse tail.

**Signal flow:**
```
input_L + input_R
    → sum to mono
    → early reflections (19-tap delay line, Size-scaled)
    → stereo ER output (alternating L/R pan per tap)
    → FDN 8-channel tail (short RT60)
    → mix ER + tail
    → dry/wet mix with original stereo input
```

**Early reflections (19 taps):**

Tap times and gains simulate a rectangular room. Size parameter (0-100%) scales all tap times linearly between small room (~3m x 2m) and large room (~10m x 8m). Each tap has a one-pole LP filter with decreasing cutoff (later reflections lose more HF, simulating air absorption).

Example tap structure (at Size=50%, 44.1kHz):

| Tap | Time (ms) | Gain | Pan |
|-----|-----------|------|-----|
| 1 | 3.3 | 0.85 | L |
| 2 | 5.5 | 0.72 | R |
| 3 | 8.7 | 0.65 | L |
| 4 | 11.2 | 0.58 | R |
| 5 | 14.8 | 0.51 | L |
| ... | ... | ... | ... |
| 19 | 93.8 | 0.03 | L |

Early Reflections parameter controls the gain of this entire stage relative to the tail.

**Late tail — 8-channel FDN:**

Householder feedback matrix: `A[i][j] = -2/N` for i!=j, `A[i][i] = 1 - 2/N` (for N=8: diagonal = 0.75, off-diagonal = -0.25). Matrix-vector multiply is O(N) via: compute sum, multiply by 2/N, subtract from each.

8 mutually prime delay line lengths (at 44.1kHz): 1447, 1559, 1619, 1723, 1847, 1951, 2063, 2179 samples. Scaled by Size parameter.

Per-channel damping via one-pole LP in each feedback path (Tone controls cutoff). Feedback gain per channel: `10^(-3 * delay_samples / (RT60 * sample_rate))`, where RT60 is derived from Decay (0.2s to 2.0s for room).

### Spring — Dispersive Allpass Cascade in Waveguide Loop

Based on Abel/Berners/Costello/Smith (AES 2006). The characteristic spring "boing" and metallic chirp come from dispersion: different frequencies travel at different speeds through the spring. This is modeled with cascaded allpass filters.

**Signal flow:**
```
input_L + input_R
    → sum to mono
    → soft-clip saturation (Dwell controls input gain)
    → 3 parallel spring waveguide models (different lengths/dispersions)
    → springs mixed to stereo (spring 1+2 → L, spring 2+3 → R)
    → dry/wet mix with original stereo input
```

**Single spring waveguide:**
```
input → dispersion cascade (N allpass sections)
      → pure delay (tau_forward, 5-15ms)
      → loss LP filter (cutoff ~4kHz)
      → feedback path:
          → return delay (tau_return, 5-15ms)
          → second dispersion pass
          → decay multiply (0.7-0.95)
          → loss LP (cutoff ~3kHz)
          → back to input sum
```

**Dispersion filter:** 20-40 cascaded first-order allpass sections. Each section:
```
y[n] = a * x[n] + x[n-1] - a * y[n-1]
```
Coefficient `a` in range 0.3-0.7. The Drip parameter scales the number of active allpass sections (more sections = more pronounced chirp/metallic character).

**Multi-spring configuration (3 springs):**

| Spring | Allpass sections | Coefficient | Forward delay | Decay |
|--------|-----------------|-------------|---------------|-------|
| 1 | 20 | 0.45 | 8ms | 0.85 |
| 2 | 30 | 0.55 | 12ms | 0.80 |
| 3 | 15 | 0.35 | 6ms | 0.90 |

Dwell parameter: input gain before the springs, with soft-clip `tanh()` saturation. Range maps 0-100% to 0.5x-3.0x gain. Higher dwell = more saturated, splashy reverb.

Decay parameter: scales the per-spring decay multiply (round-trip attenuation).

### Shimmer — Dattorro Tank + Granular Pitch Shifter in Feedback

Reuses the Dattorro tank topology from Hall as the reverb base. Adds a granular pitch shifter in the tank's feedback loop.

**Signal flow:**
```
input_L + input_R
    → sum to mono
    → Dattorro tank (same as Hall, but with shorter delays for tighter base reverb)
    → output taps → L/R output
    → feedback path:
        → granular pitch shifter (shift by Pitch interval)
        → LP filter (cutoff ~6kHz, prevents harsh HF buildup)
        → attenuate by Shimmer Amount
        → feed back into tank input
    → dry/wet mix with original stereo input
```

**Granular pitch shifter:**

Two overlapping grains with Hann window crossfade. No FFT needed — simple time-domain approach.

```rust
struct GranularPitchShifter {
    buffer: Vec<f32>,           // circular buffer ~100ms
    write_pos: usize,
    read_pos_a: f64,            // fractional read position, grain A
    read_pos_b: f64,            // fractional read position, grain B
    grain_size: usize,          // ~1024-2048 samples at 44.1kHz
    crossfade_phase: f32,       // 0.0 to 1.0
    pitch_ratio: f64,           // 1.5 for +5th, 2.0 for octave, 4.0 for +2 oct
}
```

Algorithm:
1. Write input to circular buffer at normal speed
2. Read from buffer at `pitch_ratio` speed (2.0x for octave up)
3. Two read heads offset by half a grain size
4. Hann window crossfade: `weight = 0.5 * (1.0 - cos(2*PI * phase))`
5. When a grain reaches its end, reset read position near write position
6. LP filter after pitch shift to soften artifacts

Pitch parameter mapping:
- 0-33% → ratio 1.5 (+7 semitones, perfect fifth)
- 34-66% → ratio 2.0 (+12 semitones, octave)
- 67-100% → ratio 4.0 (+24 semitones, two octaves)

Shimmer Amount: 0-100% maps to 0.0-0.90 feedback gain. At 0 it's a normal reverb. At 0.5-0.7 classic shimmer. Above 0.8 dense organ-like pads.

---

## File Structure

### New files in `crates/block-reverb/src/`

| File | Description |
|------|-------------|
| `dsp.rs` | Shared DSP primitives: DelayLine, AllpassFilter, DampingFilter, DampedCombFilter, soft-clip, cubic interpolation |
| `native_hall.rs` | Hall reverb: Modified Dattorro + Moorer ER |
| `native_room.rs` | Room reverb: Moorer TDL + 8-ch FDN Householder |
| `native_spring.rs` | Spring reverb: Dispersive allpass waveguide |
| `native_shimmer.rs` | Shimmer reverb: Dattorro tank + granular pitch shifter |

### New files in `crates/adapter-gui/src/visual_config/`

| File | Description |
|------|-------------|
| `native_hall.rs` | Visual config for Hall |
| `native_room.rs` | Visual config for Room |
| `native_spring.rs` | Visual config for Spring |
| `native_shimmer.rs` | Visual config for Shimmer |

### Modified files

| File | Change |
|------|--------|
| `crates/block-reverb/src/lib.rs` | Add `mod dsp;` |
| `crates/adapter-gui/src/visual_config/mod.rs` | Register 4 new visual configs |

No changes needed to `build.rs`, `registry.rs`, `lib.rs` public API, `runtime.rs`, or `block.rs` — the auto-discovery via `MODEL_DEFINITION` handles registration automatically.

---

## Audio Mode and Processing

All 4 models implement `StereoProcessor` with `ModelAudioMode::TrueStereo`.

The `build()` function for each model:
- `AudioChannelLayout::Stereo` → returns `BlockProcessor::Stereo(Box::new(processor))`
- `AudioChannelLayout::Mono` → bail with error (TrueStereo does not accept mono input; the runtime's `build_audio_processor_for_model` handles upmix from mono chains via DualMono fallback if needed)

Note: The runtime in `engine/src/runtime.rs` already handles `TrueStereo` + `Stereo` layout. Since the chain is always stereo (per project convention), this will work directly.

---

## Visual Identity

All models use the same panel structure (consistent with other native models) but with unique color palettes:

### Hall Reverb
- **Theme:** Cathedral, grand, golden
- `panel_bg: [0x1a, 0x16, 0x10]` — deep warm brown
- `panel_text: [0xd4, 0xb8, 0x7a]` — warm gold
- `brand_strip_bg: [0x12, 0x10, 0x0a]` — darker brown
- `model_font: "Dancing Script"`

### Room Reverb
- **Theme:** Natural, wood, neutral
- `panel_bg: [0x28, 0x24, 0x1e]` — warm wood tone
- `panel_text: [0xb8, 0xaa, 0x90]` — natural beige
- `brand_strip_bg: [0x1c, 0x18, 0x14]` — darker wood
- `model_font: "Dancing Script"`

### Spring Reverb
- **Theme:** Vintage Fender, surf, teal/cream
- `panel_bg: [0x1a, 0x2e, 0x2a]` — vintage teal-green
- `panel_text: [0xd0, 0xd8, 0xc0]` — cream white
- `brand_strip_bg: [0x12, 0x22, 0x1e]` — deeper teal
- `model_font: "Dancing Script"`

### Shimmer Reverb
- **Theme:** Ethereal, cosmic, purple-blue
- `panel_bg: [0x1e, 0x16, 0x30]` — deep purple
- `panel_text: [0xb0, 0xa0, 0xd8]` — light lavender
- `brand_strip_bg: [0x14, 0x10, 0x24]` — darker purple
- `model_font: "Dancing Script"`

---

## Testing Strategy

Each model gets tests following the delay pattern:

1. **Finite value test** — Process 10,000 samples, verify no NaN/Inf in output
2. **Schema test** — Verify `model_schema()` returns valid schema with correct parameters
3. **Build test** — Verify `build()` succeeds for stereo layout and fails for mono layout
4. **Silence test** — Feed silence, verify output converges to silence (no self-oscillation)
5. **Dry signal test** — At mix=0%, verify output equals input

Global tests in `lib.rs`:
- `supported_reverb_models_expose_schema()` — all models return valid schemas
- `supported_reverb_models_build_for_stereo_chains()` — all models build for stereo

---

## Implementation Order

1. `dsp.rs` — shared primitives (all models depend on this)
2. `native_hall.rs` — Dattorro topology (foundation for Shimmer too)
3. `native_room.rs` — independent from Hall, different algorithm
4. `native_spring.rs` — independent, unique waveguide approach
5. `native_shimmer.rs` — depends on Dattorro topology from Hall (shared code via `dsp.rs`)
6. Visual configs — 4 files in `adapter-gui`
7. Tests — per-model + global

---

## References

- Dattorro, "Effect Design Part 1: Reverberator and Other Filters" (1997) — CCRMA Stanford
- Moorer, "About This Reverberation Business" (1979)
- Abel/Berners/Costello/Smith, "Spring Reverb Emulation Using Dispersive Allpass" (AES 2006)
- Parker, "Efficient Dispersion Generation Structures for Spring Reverb" (2011)
- MVerb — Open-source Dattorro implementation (GitHub)
- Signalsmith Audio — "Let's Write a Reverb" FDN tutorial
- Sean Costello / Valhalla DSP — Reverb design blog series
