# Audio Mode Audit (Phase 4d, issue #194 — absorbed #130)

> Snapshot date: 2026-05-01. Captured during the Phase 4b cutover so the
> mono/stereo declaration of every `MODEL_DEFINITION` is on record. Any
> drift below should be revisited before flipping a model's mode.

## Why this matters

`ModelAudioMode` declares **which channel layouts a model accepts** and how
the engine wires its processor into the stereo bus. Choosing the wrong mode
manifests as one of two failures:

- **`DualMono` on a natively-stereo plugin** — the mono `Lv2Processor`
  connects only 1 input + 1 output port. A plugin with 4 audio ports
  (2 in + 2 out) ends up with 2 unconnected ports, which crashes
  with **SIGSEGV** the first time the host writes audio. This is the
  exact failure mode that motivated #130.

- **`MonoToStereo` / `TrueStereo` on a mono plugin** — wastes a builder
  call (the stereo `StereoLv2Processor` connects 4 ports; the plugin only
  has 2). Doesn't crash, but spends CPU connecting unused buffers.

## The rule (short version)

| Plugin's actual ports | Builder | `ModelAudioMode` |
|---|---|---|
| 1 in / 1 out (mono)              | `lv2::build_lv2_processor*`         | `DualMono` (or `MonoOnly`) |
| 1 in / 2 out (mono → stereo)     | `lv2::build_lv2_processor*` with `audio_out_ports = [L, R]` | `MonoToStereo` |
| 2 in / 2 out (true stereo)       | `lv2::build_stereo_lv2_processor*`  | `TrueStereo` |

For native and NAM models the same rule applies but the "builder" is the
processor type your block uses. Cab and IR models are typically `DualMono`
(stereo bus, two convolvers) or `MonoToStereo` (single mono IR
broadcast to both channels).

The non-regression checklist in `CLAUDE.md` ("Stream é SEMPRE estéreo
internamente") is what makes the rule load-bearing — every stream is a
stereo bus regardless of the plugin's natural channel count.

## Catalog summary

| Mode | Total models |
|---|---:|
| `DualMono`     | 222 |
| `MonoToStereo` |  46 |
| `TrueStereo`   |   9 |
| `MonoOnly`     |   2 |
| **Sum**        | **279** |

(Counts are over `audio_mode:` declarations in `crates/block-*/src/*.rs`.
Multi-model files like `block-body/src/ir_*.rs` contribute one count
each. NAM amps with no per-model `audio_mode:` literal inherit defaults
from the engine and are not in this count.)

## LV2 plugins (105 total) — declared mode + builder used

> **Audit invariant**: the builder a `lv2_*.rs` calls and the
> `audio_mode:` it declares must be consistent with the rule above.
> Plugins that take BOTH paths conditionally (e.g. a mono fallback
> with a stereo branch when stereo input is available) appear with
> `M+S` and were verified to dispatch to the matching mode at runtime.

### Stereo builder + stereo mode (consistent ✅)

These call `build_stereo_lv2_processor*` and declare `MonoToStereo` or
`TrueStereo`. Internal connection is 2 in / 2 out — never SIGSEGVs.

- `block-amp/lv2_mda_combo` — MonoToStereo
- `block-cab/lv2_gx_ultracab` — MonoToStereo
- `block-delay/lv2_bolliedelay`, `lv2_mda_dubdelay`, `lv2_tap_doubler`, `lv2_tap_echo` — MonoToStereo
- `block-gain/lv2_caps_spicex2` — TrueStereo
- `block-gain/lv2_mda_degrade`, `lv2_mda_overdrive`, `lv2_wolf_shaper` — MonoToStereo
- `block-mod/lv2_caps_phaser2`, `lv2_harmless`, `lv2_mda_leslie`, `lv2_mda_ringmod`, `lv2_mda_thruzero`, `lv2_tap_chorus_flanger`, `lv2_tap_rotspeak`, `lv2_tap_tremolo` — MonoToStereo
- `block-pitch/lv2_mda_detune`, `lv2_mda_repsycho` — TrueStereo
- `block-reverb/lv2_caps_platex2`, `lv2_dragonfly_early`, `lv2_dragonfly_hall`, `lv2_dragonfly_plate`, `lv2_dragonfly_room`, `lv2_mda_ambience`, `lv2_mverb`, `lv2_roomy`, `lv2_tap_reverb` — MonoToStereo

### Mono builder + DualMono (consistent ✅, 1-in/1-out plugins)

These call `build_lv2_processor*` with `[in], [out]` (single port each)
and declare `DualMono`. Engine runs the plugin twice (one per channel)
to fill the stereo bus.

`block-dyn` (6): `lv2_tap_deesser`, `lv2_tap_dynamics`, `lv2_tap_limiter`, `lv2_zamcomp`, `lv2_zamgate`, `lv2_zamulticomp`
`block-filter` (10): `lv2_artyfx_filta`, `lv2_caps_autofilter`, `lv2_fomp_autowah`, `lv2_mod_hpf`, `lv2_mod_lpf`, `lv2_mud`, `lv2_tap_equalizer`, `lv2_tap_equalizer_bw`, `lv2_zameq2`, `lv2_zamgeq31`
`block-gain` (33): `lv2_bitta`, `lv2_caps_spice`, `lv2_driva`, `lv2_gx_axisface`, `lv2_gx_bajatubedriver`, `lv2_gx_boobtube`, `lv2_gx_bottlerocket`, `lv2_gx_clubdrive`, `lv2_gx_creammachine`, `lv2_gx_dop250`, `lv2_gx_epic`, `lv2_gx_eternity`, `lv2_gx_fz1b`, `lv2_gx_fz1s`, `lv2_gx_guvnor`, `lv2_gx_hotbox`, `lv2_gx_hyperion`, `lv2_gx_knightfuzz`, `lv2_gx_liquiddrive`, `lv2_gx_luna`, `lv2_gx_microamp`, `lv2_gx_paranoia`, `lv2_gx_saturator`, `lv2_gx_sd1`, `lv2_gx_sd2lead`, `lv2_gx_shakatube`, `lv2_gx_sloopyblue`, `lv2_gx_sunface`, `lv2_gx_superfuzz`, `lv2_gx_suppatonebender`, `lv2_gx_timray`, `lv2_gx_tonemachine`, `lv2_gx_tubedistortion`, `lv2_gx_valvecaster`, `lv2_gx_vintagefuzzmaster`, `lv2_gx_vmk2`, `lv2_gx_voodofuzz`, `lv2_ojd`, `lv2_satma`, `lv2_tap_sigmoid`
`block-pitch` (2): `lv2_ewham_harmonizer`, `lv2_fat1_autotune`
`block-reverb` (6): `lv2_b_reverb`, `lv2_caps_plate`, `lv2_caps_scape`, `lv2_floaty`, `lv2_shiroverb`, `lv2_tap_reflector`

### Mono builder + MonoToStereo (REVIEW REQUIRED ⚠️)

These call the **mono** builder but declare `MonoToStereo`. The engine
broadcasts the single output to both stereo channels (`Stereo([s, s])`)
rather than running the plugin twice. They work today, but the question
is whether the plugin is **actually stereo-capable** and just being
called as 1-in/1-out — in which case slipping a `build_stereo_lv2_processor`
call would unlock real stereo output.

| Plugin | Mode declared | Builder | Note |
|---|---|---|---|
| `block-amp/lv2_gx_blueamp` | MonoToStereo | mono `[in], [out]` | If natively stereo, switch to stereo builder |
| `block-amp/lv2_gx_supersonic` | MonoToStereo | mono `[in], [out]` | same |
| `block-delay/lv2_avocado` | MonoToStereo | mono `[in], [out]` | same |
| `block-delay/lv2_floaty` | MonoToStereo | mono | conflicts with reverb's `lv2_floaty` (DualMono); investigate which is the same plugin |
| `block-delay/lv2_modulay` | MonoToStereo | mono | |
| `block-delay/lv2_tap_reflector` | MonoToStereo | mono | conflicts with reverb's same name (DualMono); investigate |
| `block-gain/lv2_invada_tube` | MonoToStereo | mono | |
| `block-gain/lv2_tap_tubewarmth` | MonoToStereo | mono | |
| `block-mod/lv2_fomp_cs_chorus` | MonoToStereo | mono | |
| `block-mod/lv2_fomp_cs_phaser` | MonoToStereo | mono | |
| `block-mod/lv2_larynx` | MonoToStereo | mono | |
| `block-wah/lv2_gx_quack` | MonoToStereo | mono | |

**Decision (Phase 4b cutover):** these are NOT auto-flipped. Audio output
that "works" today is sacred per the non-regression checklist
(`CLAUDE.md` invariant 10: volume per stream is immutable without
explicit user request). Each entry needs an A/B audition before flipping.

## Recommended follow-ups (out of scope of #194)

1. Open a focused issue per LV2 in the "review required" table to A/B
   audition the current mono path vs a stereo build.
2. Add a runtime introspection test that loads each LV2's TTL,
   counts `lv2:AudioPort` of each direction, and asserts `audio_mode`
   matches the rule. This needs `lilv` access.

## Static contract test

A regression test could pin every `lv2_*.rs` against a `(builder, mode)`
fixture that mirrors this audit. Adding a new plugin would force the
contributor to update the fixture explicitly. Tracked but not landed in
this slice — the audit doc is the contract until the runtime
introspection test exists.
