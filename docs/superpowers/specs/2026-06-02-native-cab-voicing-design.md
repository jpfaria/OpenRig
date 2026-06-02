# Native cabinet voicing — distinct per-model biquad fingerprint

Issue #620.

## Problem

All native CAB models (`brit_4x12`, `vintage_1x12`, `american_2x12`) share one
`crates/block-cab/src/native_core.rs` engine built from one-pole filters, with
only timid per-model deltas (`high_cut_scale` 0.78–1.0 + subtle resonance/air).
Switching the cabinet model barely changes the tone.

Measured with a deterministic render (identical knobs, identical broadband
signal): pairwise relative RMS difference is only **5–9 %**:

- brit vs vintage 5.3 %, brit vs american 6.7 %, vintage vs american 9.0 %.

## Goal

Each native cabinet has a distinct, audibly different voice — while keeping
**~0 added latency** (chosen over IR convolution, which would add ~1.3 ms per
the partitioned FFT engine in `crates/ir`). Existing knobs and parameters stay;
only the engine underneath changes.

## Approach

Replace the one-pole chain with a **cascade of `block_core::BiquadFilter`**
(reused, not duplicated — RBJ cookbook, already has coefficient ramping that
suppresses the parameter-change click from #358). Each model is described by a
`CabVoiceProfile` — a fixed fingerprint of biquad stages:

1. **Body high-pass** — removes sub-cabinet rumble (`HighPass`, ~80–120 Hz).
2. **Speaker rolloff** — the dominant cabinet trait (`LowPass` with Q, ~4–6 kHz),
   resonant so the knee has character instead of a gentle slope.
3. **Cone/low bump** — `Peak` ~100–220 Hz, the thump that separates a 4x12 from
   a 1x12.
4. **Mid dip** — `Peak` (negative gain) ~1.2–2.5 kHz, the classic guitar-cab
   scoop; its centre/depth is a strong per-model differentiator.
5. **Presence peak** — `Peak` or `HighShelf` ~2.5–5 kHz, the bite/air on axis.

The per-stage frequencies, gains and Qs differ enough between models that the
voicing is obviously distinct, not a 0.78-vs-1.0 scaling.

### Knob mapping (parameters unchanged)

| Knob | Drives |
|---|---|
| `low_cut_hz` | body high-pass freq |
| `high_cut_hz` | speaker rolloff low-pass freq (× model scale) |
| `resonance` | low bump gain + rolloff Q |
| `air` | presence stage gain |
| `mic_position` | shifts presence freq + rolloff (on/off axis) |
| `mic_distance` | room delay tap + room low-pass (kept as today) |
| `room_mix` | dry/room blend (kept as today) |
| `output` | output gain (kept as today) |

The room/delay section (`DelayTap`, `room_low_pass`) is retained as-is.

## Per-model fingerprints (starting point, tuned during GREEN)

| Model | Rolloff | Low bump | Mid dip | Presence |
|---|---|---|---|---|
| `brit_4x12` (Celestion-ish, tight/aggressive) | ~5.0 kHz, high Q | 150 Hz +3 dB | 2.0 kHz −4 dB | 3.5 kHz +4 dB |
| `vintage_1x12` (small, warm/boxy) | ~4.0 kHz, med Q | 110 Hz +2 dB | 1.4 kHz −5 dB | 3.0 kHz +2 dB |
| `american_2x12` (scooped, bright/clean) | ~6.0 kHz, low Q | 100 Hz +2 dB | 800 Hz −5 dB | 4.5 kHz +5 dB |

No brand names in code (zero-coupling rule) — these are descriptive targets only.

## Invariants

- **Audio thread**: biquads are alloc-/lock-free in `process_sample`; all
  `BiquadFilter`s are built in `NativeCabProcessor::new` (setup), never in the
  callback. No syscalls/IO.
- **Latency**: biquads add no algorithmic delay → round-trip unchanged.
- **Determinism**: pure f32 IIR; golden render tests pin output within tolerance.
- **CPU**: ~5 biquads/sample/channel — cheaper than any convolution; safe on
  Orange Pi.
- **Stream is always stereo**: `DualMono` audio_mode preserved (independent L/R
  processors, as today).

## Testing (TDD, RED first)

1. **RED** (`crates/block-cab/tests/issue_620_cab_voicing.rs`): render the same
   broadband signal through each model with identical knobs; assert pairwise
   relative RMS diff `> 0.20`. Fails today (5–9 %).
2. **Response shape**: per model, assert the magnitude response has its
   characteristic high-frequency rolloff (e.g. −X dB at 8 kHz vs 1 kHz) and a
   presence bump — so "different" also means "cabinet-shaped", not just noisy.
3. **Regression**: existing `lib_tests.rs` (schema, mono/stereo build) stay green;
   add a golden-sample determinism check.

## Files

- `crates/block-cab/src/native_core.rs` — swap engine to biquad cascade; add
  `CabVoiceProfile` stage list; map knobs onto stages.
- `crates/block-cab/src/native_{brit_4x12,vintage_1x12,american_2x12}.rs` —
  replace `NativeCabProfile` constants with the new fingerprint.
- `crates/block-cab/tests/issue_620_cab_voicing.rs` — RED repro + response/golden.
- `docs/blocks-catalog.md` — note the per-model voicing (same commit).
