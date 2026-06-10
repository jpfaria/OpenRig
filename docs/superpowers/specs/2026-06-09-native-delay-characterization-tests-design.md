# Native delay — characterization tests that prove each model fulfils its purpose

Issue #388.

## Problem

The six native delay models (`analog_warm`, `digital_clean`, `slapback`,
`reverse`, `modulated`, `tape_vintage`) each ship **the same four tests**, copied
verbatim with only the name prefix changed:

- `<model>_outputs_finite_values`
- `process_frame_silence_output_is_finite`
- `process_frame_sine_output_is_finite`
- `process_block_1024_frames_all_finite`

All four only assert the output is not `NaN`/`Inf`. None proves there *is* a
delay, what the delay time is, or that the model has the character its name
promises. Because nothing tests behaviour, two models drifted into being toys:

- **`slapback` has byte-for-byte the same DSP as `digital_clean`** — both call
  `process_simple_delay(line, input, feedback, mix)`. Only the default knob
  values differ. Switching from Digital Clean to Slapback changes nothing in the
  algorithm.
- **`analog_warm` and `tape_vintage` have no non-linearity** — "analog" and
  "tape" both imply saturation/warmth, but the code only applies a one-pole
  low-pass. The repeats are sterile.

So today **zero** native delay models have a test that proves they work.

## Goal

Every native delay model has a deterministic test that proves it fulfils its
defining proposal. Where a model fails its own characterization (Slapback,
Analog Warm, Tape Vintage), fix the DSP so it genuinely earns its name. Done
red-first, one model per PR, with **no regression to latency, determinism, or
the real-time audio-thread invariants** (CLAUDE.md).

## Approach

### Shared probe (`dsp_probe`, test-only)

Add `crates/block-delay/src/dsp_probe.rs`, compiled `#[cfg(test)]` only, with
reusable measurements. It is **never** linked into the audio path — `realfft`
is added under `[dev-dependencies]` only.

- `render_mono(proc, input) -> Vec<f32>` — drive a `MonoProcessor` sample by
  sample over an input buffer.
- `impulse_echoes(signal, sr) -> Vec<Echo>` — locate echo peaks (lag in samples,
  peak amplitude) from an impulse render.
- `spectral_centroid(segment, sr) -> f32` — brightness of a segment via
  `realfft`. Used to prove darkening/tone.
- `harmonic_energy(signal, f0, sr) -> f32` — energy at `2·f0 + 3·f0` relative to
  `f0` after a pure-sine input. Used to prove (or disprove) saturation.
- `lag_modulation(signal, sr) -> LagMod` — sliding cross-correlation lag over
  time → `{ variance, dominant_rate_hz }`. Used to prove LFO / wow-flutter.
- `rms_difference(a, b) -> f32` — relative RMS between two renders. Used to prove
  one model is audibly distinct from another on identical knobs.

Each measurement is validated against a known signal (not a tautology) so a
green test means the property is actually present.

### Per-model proposal → what the test proves

| Model | Proposal | Assertions | Today |
|---|---|---|---|
| `digital_clean` | Pristine repeats, no colour | echoes at multiples of `time_ms` (±1 sample); `centroid(echo2) ≈ centroid(echo1)` (no darkening); `harmonic_energy ≈ 0` (linear); `peak2/peak1 ≈ feedback` | PASS (pin) |
| `slapback` | One short, distinct, analog-flavoured slap | second echo ≥ 12 dB below first at default; `centroid(echo) < centroid(dry)` (darker than input); **`rms_difference(slapback, digital_clean)` ≥ threshold on identical knobs** | **FAIL** — identical to digital_clean |
| `analog_warm` | BBD: repeats darken + warmth | `centroid(echoN+1) < centroid(echoN)` (monotonic darkening); **`harmonic_energy` above clean baseline with a hot input** (saturation) | **FAIL** — no saturation |
| `tape_vintage` | Wow/flutter + tape tone + magnetic saturation | `lag_modulation.variance > 0` with two-rate wobble; `centroid(echo) < centroid(dry)`; **`harmonic_energy` above clean baseline** (saturation) | **FAIL** — no saturation |
| `modulated` | LFO-modulated delay time | `lag_modulation.dominant_rate_hz ≈ rate_hz` and `variance > 0` when `depth>0`; `variance ≈ 0` when `depth=0` | PASS (pin) |
| `reverse` | Echo is the time-reversed segment | with an asymmetric probe (rising ramp burst), `corr(echo, reversed_input) > corr(echo, forward_input)` | PASS (pin) |

Tolerances are tuned during GREEN against the real renders.

### DSP fixes (the three toys)

- **`shared::soft_saturate(x, drive)`** — a `tanh`-based saturator, branch-free,
  alloc-/lock-free, added to `shared.rs` for reuse.
- **`slapback`** — replace the `process_simple_delay` passthrough with a
  dedicated processor: a single-tap echo with one-pole HF roll-off on the tap
  and light `soft_saturate`, giving the dark analog slap it is named for and
  making it provably distinct from `digital_clean`.
- **`analog_warm`** — apply `soft_saturate` in the feedback path (BBD-style)
  on top of the existing low-pass.
- **`tape_vintage`** — apply `soft_saturate` (magnetic-style) in the feedback
  path on top of the existing wow/flutter + tone.

No brand names in code (zero-coupling rule); pedal references in this doc are
descriptive targets only.

## Invariants

- `process_sample` stays alloc-/lock-/syscall-free; `soft_saturate` is a pure
  arithmetic step.
- **No added latency** — all changes are time-domain in the existing delay line;
  no convolution, no look-ahead.
- `dsp_probe` and `realfft` are **test-only** (`#[cfg(test)]` /
  `[dev-dependencies]`), never in the audio path.
- Determinism preserved; `cargo build` stays warning-free.
- `digital_clean`, `modulated`, `reverse` behaviour is **pinned, not changed** —
  their tests must pass without touching production code.

## Test plan

Red-first per CLAUDE.md. For each toy model the failing assertion (saturation /
distinctness) is observed RED before the DSP change, then GREEN after. For the
three honest models the characterization test pins existing-correct behaviour;
each metric is sanity-checked so the green is meaningful, not vacuous. The four
boilerplate `*_finite*` tests stay (cheap denormal/NaN guard).

## Build sequence (one model per PR)

1. **`dsp_probe` harness + Digital Clean** — add the test module + `realfft`
   dev-dep; characterization test for Digital Clean (baseline, no DSP change).
2. **Slapback** — RED (identical to digital_clean) → dedicated processor → GREEN.
3. **Analog Warm** — RED (no saturation) → `soft_saturate` in feedback → GREEN.
4. **Tape Vintage** — RED (no saturation) → magnetic `soft_saturate` → GREEN.
5. **Modulated** — characterization test (pin), no DSP change.
6. **Reverse** — characterization test (pin), no DSP change.

## Out of scope

- New delay types from #388's roadmap (multi-tap, ping-pong, BBD/EP-3 emulations,
  granular) — separate PRs after the six existing models are proven.
- True stereo / cross-feedback (the DualMono structure stays as-is here).
- Catalog/LV2/VST3 work (lives in master #349).
