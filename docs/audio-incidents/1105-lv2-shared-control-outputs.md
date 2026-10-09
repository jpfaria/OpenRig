# 1105 — LV2 compressor boosts a signal below its threshold

## Symptom (measured)

Ardour's ACE Compressor and Expander (stereo), with their defaults (threshold
0 dB, no makeup), raised a 440 Hz sine at −13.5 dBFS RMS to −5.2 dBFS
(compressor) and −3.1 dBFS (expander). Found while packaging them; no rig
report. Before the patch below they also crashed (SIGSEGV) on build.

## Cause (CONFIRMED)

- **Shared output slots.** The LV2 processors connected every output control
  port (gain-reduction meter, input level, output level) to one scratch
  buffer. a-comp/a-exp read their gain-reduction port back as state at the
  start of `run()`; the input-level meter written after it (≈ −10 dB) came back
  as a negative gain reduction, i.e. a boost. Fix: each output control port
  gets its own slot (`crates/lv2/src/processor.rs`,
  `crates/lv2/src/stereo_processor.rs`).
- **Ports touched in `activate()`.** a-comp/a-exp write those output ports in
  `activate()`; OpenRig connects ports after `activate()` (allowed by LV2), so
  the writes hit null. The `ardour-ace` recipe patches the writes out
  (`scripts/recipes/ardour-ace-activate-ports.patch`); `run()` writes the
  same ports every cycle.

## Reproduce

`cargo test -p lv2 --test issue_1105_control_outputs_own_slots` with the
`ardour_a_comp` / `ardour_a_exp` binaries for this platform in
`plugins/source/lv2/`. Before the fix: out −5.2 dBFS for in −13.5 dBFS; after:
equal within 0.5 dB.
