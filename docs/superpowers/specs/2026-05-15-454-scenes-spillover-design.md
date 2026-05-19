# #454 — Scenes per preset + spillover — Design Spec

> Sub-issue de #436. Substitui #321. **Sub-projeto de áudio crítico.**
> Depende de #449 (modelo) + #451 (RigRuntime / swap lock-free).

## Goal

Up to 8 scenes per preset, Helix-Snapshot style: a scene is **only the diff**
over the base preset (bypass set + values of *marked* params). Switching a
scene (or preset) builds the new pipeline off the audio thread, swaps
lock-free, crossfades, and lets the old pipeline's tail decay (spillover).
Backward-compat: a preset with no scenes loads as a single "Default" scene.

## Model (extends `RigPreset`)

```rust
pub struct RigPreset {
    pub blocks: Vec<AudioBlock>,
    /// Marked params the scenes control. Key: "<block-id>.<param-id>".
    /// Anything not listed is fixed by the preset (Helix Snapshot rule).
    pub scene_params: Vec<String>,          // serde: scene-params
    /// 1..=8. Empty ⇒ treated as a single Default scene (index 1).
    pub scenes: BTreeMap<usize, RigScene>,  // serde: scenes
}

pub struct RigScene {
    pub label: Option<String>,
    /// block-id → bypassed-in-this-scene.
    pub bypass: BTreeMap<String, bool>,
    /// "<block-id>.<param-id>" → value. Must be a subset of scene-params.
    pub params: BTreeMap<String, f32>,
}
```

`RigProject::validate` additionally rejects: scene index ∉ `1..=8`;
`scene.params` key not in `scene_params`; `bypass`/param key naming an unknown
block. Backward-compat handled by `RigPreset::scene_or_default(idx)` which
returns an empty scene when `scenes` is empty.

## Pure scene resolution (hardware-free, TDD)

`apply_scene(&RigPreset, idx) -> Vec<AudioBlock>`:
clone `blocks`; for the resolved scene set each block `enabled = !bypass`;
for each `scene_params` key present in `scene.params`, override that block
param. Params **not** marked are untouched (fixed by preset). Determinism:
`BTreeMap` ordering.

## Engine — scene/preset switch (reuses #451)

`RigRuntime::switch_scene(input, idx)` mirrors `switch_preset`: rebuild that
input's synthetic `Chain` from `apply_scene(active_preset, idx)` and upsert via
the **proven lock-free in-place path** (`Arc<ChainRuntimeState>` preserved,
build off the brief lock; existing per-segment cosine fade-in = click-free).

## Spillover (the RT-critical piece)

Same swap mechanism as #451. The old pipeline's delay/reverb tail must decay
instead of being cut. SPSC-safe design: the crossfade is composed **inside one
segment** (one ring producer preserved) — the segment holds an optional
`outgoing` `InputProcessingState`; during the window `process_single_segment`
runs new + old, equal-power crossfades, feeds the old one silence (tail only),
and drops it when its envelope reaches zero. Mirrors the existing per-block
`FadeState` dry/wet crossfade, lifted to segment granularity. Transient extra
CPU during the window only (hierarchy: CPU < sound+stability; bounded).

**Gate (every step):** `volume_invariants`, `stream_isolation`,
`audio_signal_integrity`, plus a new golden spillover test asserting (a) no
click on switch, (b) the previous scene's tail is audible during the window,
(c) bit-stable steady state. A volume-invariant break ⇒ the source is wrong.

## Out of scope

Lock-free param swap without a new stream (old #321 — discarded). MIDI/footswitch.

## Tasks

- [ ] T1 — model: `scene_params`, `scenes`, `RigScene`, validation,
  `scene_or_default`. Backward-compat. TDD.
- [ ] T2 — pure `apply_scene` (bypass + marked-param override, order/determinism). TDD.
- [ ] T3 — persistence: scenes/scene-params round-trip in `rig_yaml`; preset
  without scenes loads as Default. TDD.
- [ ] T4 — `RigRuntime::switch_scene` via the #451 lock-free path; isolation +
  volume invariants green. TDD.
- [ ] T5 — spillover: in-segment decaying `outgoing`; golden spillover test;
  full audio gate green.
- [ ] T6 — docs + `./scripts/qa.sh` green + push + comment #454.
