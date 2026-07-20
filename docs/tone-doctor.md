# Tone Doctor (#791)

Reference-free tone diagnosis. It answers two questions a spectrum analyzer
cannot: *is this chain's tone unhealthy?* and, when it is, *which block caused
it and what do I turn?*

The advantage is structural: because the chain and its deterministic offline
render belong to OpenRig, Tone Doctor can measure the chain with blocks added
and removed and prove causation. A third-party analyzer plugin sees only the
final signal and cannot re-render the chain without its neighbours.

This document covers the two layers that exist today. The live traffic-light
surface and the on-screen panel are tracked in #791 and not yet implemented.

## Layer 0 — descriptors (`feature-dsp::tone_descriptors`)

Pure, deterministic DSP: a rendered buffer in, a small set of scalar
descriptors out. No state, no smoothing, no UI.

| Descriptor | Meaning |
|---|---|
| `rms_dbfs`, `peak_dbfs` | level of the buffer |
| `crest_db` | peak − RMS; low = compressed/clipped, ~12–20 dB = a clean pluck |
| `clip_fraction` | fraction of samples pinned at the ±1.0 rail |
| `fizz_ratio` | presence-band (3–8 kHz) power ÷ note-body (200 Hz–2 kHz) power |
| `mud_ratio` | low-mid (160–500 Hz) power ÷ total power |

Band energy is a Welch-averaged power spectrum (Hann window, 50 % overlap), so
a multi-second take collapses to one stable estimate.

`ToneDescriptors::symptom()` maps the descriptors to a dominant `Symptom`
(`Ok`, `Fizz`, `Mud`, `Clipping`); clipping wins over spectral tilt because it
is the most audible failure.

### Thresholds are provisional

The cut-offs (`FIZZ_RATIO_LIMIT`, `MUD_RATIO_LIMIT`, `CLIP_FRACTION_LIMIT`) are
set conservatively so a clean signal never trips them — they separate "clean"
from "carries real presence-band / low-mid / rail-pinned content". The
*musical* boundary between wanted and unwanted colour (a fuzz is meant to be
buzzy) needs real recordings and the player's ear to calibrate, and that tuning
is deferred. Treat the current symptom classification as a heuristic, not a
verdict.

## Layer 2 — blame by ablation (`engine::tone_doctor`)

`diagnose(chain, sample_rate, input, block_size) -> Diagnosis` takes a slice of
the player's own DI and re-renders the chain offline through
`engine::offline::render_chain`:

- **Growth curve** — render with the enabled processing blocks turned on one at
  a time, in signal order. The offending descriptor is measured as each block
  joins; the symptom is *born* at the first prefix where it crosses the limit,
  and that prefix names the block.
- **Bypass confirmation** — re-render the full chain with just that block
  disabled. If the symptom clears, the blame is causal (`bypass_resolved =
  true`); if not, the cause is a cross-block interaction (e.g. a drive slamming
  the amp's input) and the report says so.

`Diagnosis` carries the full-chain symptom and descriptors, the growth `curve`
(one `GrowthStage` per enabled processing block), the `culprit` block index (or
`None` for a healthy chain), and `bypass_resolved`.

I/O endpoint blocks and blocks the user already bypassed are never candidates —
a block that is not in the signal cannot be the culprit.

### Invariants

Everything runs offline and reuses the existing deterministic render path:

- No new block in the signal graph, no per-block instrumentation, no descriptor
  maths on the audio thread — invariants #7 and #8 are untouched by
  construction.
- Per-chain isolation (#4) holds: every render sees only its own chain.
- Deterministic (#9): the same samples always yield the same descriptors, so
  the tests pin behaviour with synthetic fixtures of known spectral content.

## Symptom → parameter suggestion (`engine::tone_doctor_suggestion`)

`suggest(chain, &diagnosis) -> Option<Suggestion>` maps the diagnosed symptom
and culprit block to a concrete knob and a proposed value — the "what do I
turn" half. It walks a per-symptom priority list of parameter paths (fizz →
`presence`/`treble`/`tone`…, mud → `mids`/`bass`…, clipping →
`level`/`master`…), picks the first float-range parameter the culprit actually
exposes, and nudges it toward health by a quarter of its range, clamped to the
valid range. It never invents a parameter a block lacks; NAM/Insert/Select
blocks yield no suggestion. Applying is the caller's job — a
`SetBlockParameterNumber` command with the suggestion's block, path and value.

## Transport parity — objective report query

Layer 3 is exposed read-side for every transport via
`QueryKind::ChainQualityReport { chain }`, resolved off-frontend from the
published snapshot (`application::query_chain_quality::chain_quality_report`).
MCP serves it at `openrig://chains/{chain}/quality` as a JSON envelope
(`{"quality": { thd_n, noise_floor_dbfs, peak_dbfs, rms_dbfs,
dynamic_range_db, clip_fraction }}`). The GUI and console adapters resolve the
same query against their own project, so all transports see identical numbers.

## UI (`adapter-gui`)

A stethoscope button sits in every chain header (`chain_row.slint` main page +
`compact_chain_view_header.slint`), left of the DI fone. Clicking it seeds the
`ToneDoctorState` global and opens an inline overlay panel (`tone_doctor_panel`
via `tone_doctor_overlay`, rendered at the window root — never a `PopupWindow`,
per #749/#761), scoped to that chain.

The panel's **Diagnose** button runs the offline ablation over the chain's
selected DI (`di_loop_source_for_chain` → `di_loader::load_di_loop` →
`DiPcm::stereo_frames`), then shows a symptom traffic light, the culprit block,
and the suggested fix as `Tone 70 → 45` with an **Apply** button that dispatches
the existing `SetBlockParameterNumber` command. When no DI is selected it shows
an amber "select a DI" line. The glue lives in `tone_doctor_compact_wiring`
(both the main page and the compact window reuse it); the diagnosis→view and
suggestion→command mapping is the pure, unit-tested `tone_doctor_wiring`.

Strings are translated across all nine locales. The dynamic symptom words
(Fizz/Mud/Clipping/OK) are the descriptor names, kept as-is for now.

## Not yet built (tracked in #791)

- Layer 1 *live* traffic light reading the output tap while playing (today the
  light updates on an explicit Diagnose run, not continuously).
- An interaction test (`i-slint-backend-testing`) driving the button → panel →
  Apply click path headlessly.
