# Guitar EQ Implementation Design

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a native Guitar EQ filter block that cuts problematic low and high frequencies for guitar, with two independent intensity controls.

**Architecture:** Single new source file `native_guitar_eq.rs` in `crates/block-filter/src/`. The `build.rs` auto-registration pattern picks it up automatically — no registry edits needed. Uses existing `BiquadFilter` with `LowShelf` and `HighShelf` kinds from `block-core`.

**Tech Stack:** Rust, `block-core::BiquadFilter`, `block-filter` crate pattern.

---

## Context

Guitar signals have two well-known problematic frequency regions:
- **Below ~80Hz** — low-end rumble, stage noise, handling noise, room resonance. Adds mud without musical content for guitar.
- **Above ~8kHz** — pick attack fizz, hiss, harsh upper harmonics. Makes the signal sound brittle and noisy.

Cutting these ranges makes guitar signals sit better in a mix and sound cleaner through an amp chain. The cutoff frequencies are fixed (industry-standard for guitar), but the intensity of each cut is independently adjustable so the player can tailor to their instrument and style.

---

## Parameters

| ID | Label | Default | Range | Step | Unit |
|----|-------|---------|-------|------|------|
| `low_cut` | Low Cut | 100 | 0–100 | 1 | % |
| `high_cut` | High Cut | 100 | 0–100 | 1 | % |

- `low_cut = 0%` → low shelf gain = 0 dB (no effect below 80Hz)
- `low_cut = 100%` → low shelf gain = −12 dB below 80Hz
- `high_cut = 0%` → high shelf gain = 0 dB (no effect above 8kHz)
- `high_cut = 100%` → high shelf gain = −12 dB above 8kHz

**Formula:** `gain_db = -(value / 100.0) * 12.0`

---

## DSP Design

Two `BiquadFilter` in series:

```
input → [LowShelf @ 80Hz, Q=0.707] → [HighShelf @ 8kHz, Q=0.707] → output
```

- **Low shelf at 80Hz, Q=0.707 (Butterworth)** — smooth rolloff below 80Hz, musical and natural sounding
- **High shelf at 8kHz, Q=0.707 (Butterworth)** — smooth rolloff above 8kHz
- Filters are rebuilt when parameters change (stateless construction pattern, same as `ThreeBandEq`)

---

## Model Definition

| Field | Value |
|-------|-------|
| `id` | `native_guitar_eq` |
| `display_name` | `Guitar EQ` |
| `brand` | `""` (native) |
| `backend_kind` | `FilterBackendKind::Native` |
| `audio_mode` | `ModelAudioMode::DualMono` |
| `supported_instruments` | `block_core::GUITAR_ACOUSTIC_BASS` |

---

## Files

| Action | Path |
|--------|------|
| Create | `crates/block-filter/src/native_guitar_eq.rs` |
| Modify | `assets/blocks/metadata/en-US.yaml` |
| Modify | `assets/blocks/metadata/pt-BR.yaml` |

`build.rs` auto-registers the model — no other files need to change.

---

## Metadata

**en-US.yaml:**
```yaml
native_guitar_eq:
  description: "Guitar EQ — cuts low-end rumble below 80Hz and high-frequency fizz above 8kHz. Two independent controls let you dial in how much of each cut to apply."
  license: "Proprietary - OpenRig"
  homepage: "https://github.com/jpfaria/OpenRig"
```

**pt-BR.yaml:**
```yaml
native_guitar_eq:
  description: "Guitar EQ — corta o grave abaixo de 80Hz e o agudo acima de 8kHz. Dois controles independentes permitem ajustar a intensidade de cada corte."
  license: "Proprietário - OpenRig"
  homepage: "https://github.com/jpfaria/OpenRig"
```

---

## Implementation Pattern

Follow exactly the same structure as `native_eq_three_band_basic.rs`:

```rust
pub const MODEL_ID: &str = "native_guitar_eq";
pub const DISPLAY_NAME: &str = "Guitar EQ";

pub struct GuitarEq {
    low_shelf: BiquadFilter,
    high_shelf: BiquadFilter,
}

impl GuitarEq {
    pub fn new(low_cut: f32, high_cut: f32, sample_rate: f32) -> Self {
        let low_gain_db  = -(low_cut  / 100.0) * 12.0;
        let high_gain_db = -(high_cut / 100.0) * 12.0;
        Self {
            low_shelf:  BiquadFilter::new(BiquadKind::LowShelf,  80.0,   low_gain_db,  0.707, sample_rate),
            high_shelf: BiquadFilter::new(BiquadKind::HighShelf, 8000.0, high_gain_db, 0.707, sample_rate),
        }
    }
}

impl MonoProcessor for GuitarEq {
    fn process_sample(&mut self, input: f32) -> f32 {
        let x = self.low_shelf.process(input);
        self.high_shelf.process(x)
    }
}
```

Parameters use `float_parameter` (not `curve_editor_parameter`) since there is no curve editor widget for a simple percentage knob.

`supported_instruments`: `block_core::GUITAR_ACOUSTIC_BASS`
