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

## Not yet built (tracked in #791)

- Layer 1 live traffic light reading the existing output tap.
- The symptom → parameter suggestion and its `Apply` `Command`.
- The `Query`/`QueryKind` for GUI/MCP/gRPC parity.
- Layer 3 objective quality report (THD+N/SNR/frequency response; was #609).
- The chain-header button and overlay panel.
