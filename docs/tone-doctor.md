# Tone Doctor (#791, #809)

Reference-free tone diagnosis. It answers two questions a spectrum analyzer
cannot: *is this chain's tone unhealthy?* and, when it is, *which block caused
it and what do I turn?*

The advantage is structural: because the chain and its deterministic offline
render belong to OpenRig, Tone Doctor can measure the chain with blocks added
and removed and prove causation. A third-party analyzer plugin sees only the
final signal and cannot re-render the chain without its neighbours.

Since #809 the verdict is **genre-aware**: what counts as too much fizz for a
blues-rock tone is normal for grunge, so the limits come from a table
calibrated against real reference recordings instead of one global constant.

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
| `boom_ratio` | low-end (40–120 Hz) power ÷ total power (#809) |

Band energy is a Welch-averaged power spectrum (Hann window, 50 % overlap), so
a multi-second take collapses to one stable estimate.

`ToneDescriptors::symptom()` maps the descriptors to a dominant `Symptom`.
`Clipping` short-circuits everything because it is the most audible failure;
otherwise each candidate symptom is scored by how far it is past its limit,
normalized by that limit, and the largest positive score wins — no positive
score means `Ok`.

| Symptom | Descriptor | Direction | Meaning |
|---|---|---|---|
| `Fizz` | `fizz_ratio` | excess | presence band dominates the note body |
| `Mud` | `mud_ratio` | excess | low-mid buildup |
| `Boomy` | `boom_ratio` | excess (#809) | low-end buildup below the note fundamental |
| `Thin` | `mud_ratio` | **deficit** (#809) | not enough low-mid body for the genre |
| `Squash` | `crest_db` | **deficit** (#809) | over-compressed — peaks flattened into the RMS |
| `Clipping` | `clip_fraction` | excess | samples pinned at the rail |

The two deficit symptoms are the point of #809: an excess-only doctor could
only ever tell you to turn something *down*.

### Genre-calibrated limits (#809)

Each symptom's limit comes from a per-genre `SymptomLimits` row. Picking a
genre in the panel selects that row; picking none uses the conservative global
defaults, which keep the original #791 behaviour.

**The deficit floors default to zero, which disables them** — so `Thin` and
`Squash` can only ever fire when you have selected a genre. Without a genre,
Tone Doctor still only reports excesses.

The table lives in `assets/tone-profiles/profiles.yaml`, is compiled into the
binary, and was derived offline from genre-labelled isolated-guitar reference
stems: excess limits are the corpus **p90** for that genre, deficit floors the
**p10**. Genres measured from fewer than six stems are marked `provisional` in
the file (the runtime ignores the marker — the UI does not surface it). Full
methodology, the derived numbers, and the honest caveats are in
[`development/tone-doctor-calibration.md`](development/tone-doctor-calibration.md).

The spread is why one global constant could not work: `fizz` for grunge
calibrates ~30× higher than for blues-rock.

⚠️ A `harsh` (8–16 kHz) axis was built and then **removed**: a guitar cab rolls
off above ~5–6 kHz, so that band measured ~0 across the whole corpus. It was a
dead axis, not a strict one — brightness is already carried by `fizz`. Do not
re-add it.

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

### Measured, bidirectional auto-fix (`engine::tone_doctor_fix`, #809)

The panel does not ship the unverified nudge above. It calls
`measure_fix_with_limits`, which **proves** the fix instead of guessing it: for
the culprit's candidate knob it sweeps 25 / 50 / 75 / 100 % of the distance to
the range end, and each trial is a real re-render plus a re-measure. The first
value that actually reads healthy wins; if none does within its 8-render
budget it returns nothing — an honest "no fix on this block" rather than a
plausible-looking knob move.

*Bidirectional* is what the deficit symptoms needed: an excess (Fizz, Mud,
Boomy, Clipping) still sweeps **downward only**, but a deficit (Thin, Squash)
tries **both directions**, since the cure for a thin tone is to add body, not
remove it. Health is checked direction-aware — at or above the floor for a
deficit, below the limit for an excess.

Applying dispatches `SetBlockParameterNumber`, preceded by
`SetBlockParameterBool` on the group's `.enabled` path when the target knob
sits in a group that is switched off.

## Transport parity — the doctor itself is on the command bus

The diagnosis and its fix are `Command`s, not GUI behaviour:

- `ToneDoctorCommand::DiagnoseChainTone { chain, genre, seconds }` — picks the
  signal (the chain's loaded DI loop when there is one, otherwise the live input
  the adapter registered via `LocalDispatcher::attach_tone_doctor_input`), runs
  the ablation off-thread, and lands `Event::ChainToneDiagnosed { chain, report }`
  through `poll_async_results`. With neither source the command errors instead of
  guessing. The dispatcher caches the verdict per chain.
- `ToneDoctorCommand::ApplyToneDoctorFix { chain }` — replays the cached fix as
  ordinary `SetBlockParameterBool` (the group gate, when present) +
  `SetBlockParameterNumber`, so every observer sees the same events a knob turn
  emits, then emits `Event::ChainToneFixApplied`.

The verdict is read back with `QueryKind::ChainToneReport { chain }` (MCP:
`openrig://chains/{chain}/tone`), served from dispatcher state — MCP reads
exactly the report the GUI panel is showing rather than re-rendering its own.
`application::tone_doctor_report` owns the report shape and the report → commands
mapping; it is transport-agnostic and names blocks by `BlockId`, never by index.

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

The panel's **Diagnose** button dispatches `DiagnoseChainTone`; the verdict
arrives on the frontend drain as `Event::ChainToneDiagnosed` and
`tone_doctor_events` paints it on whichever window has the panel open. It shows
a symptom traffic light, the culprit block, and the suggested fix as
`Tone 70 → 45`, with an **Apply** button that dispatches `ApplyToneDoctorFix`
and then re-syncs the chain's live runtime (#808). With no DI and no live chain
the command errors and the panel says there is nothing to analyse. The glue
lives in `tone_doctor_compact_wiring` (both the main page and the compact window
reuse it); the report → view mapping is the pure, unit-tested
`tone_doctor_wiring`.

Strings are translated across all nine locales. The dynamic symptom words
(Fizz/Mud/Boomy/Thin/Squash/Clipping/OK) are the descriptor names, kept as-is
for now. The culprit is shown by its catalog display name, not its raw model
identity.

### Genre selector (#809)

Between the take-length chips and the **Diagnose** button sits a searchable
genre select listing the 15 calibrated genres, plus a `—` entry meaning *no
genre → global defaults*. The genre keys are shown verbatim
(`alternative-metal`, `blues-rock`, `grunge`, `heavy-metal`, `mpb`,
`punk-rock`, …); there is no separate display label. Typing filters the list.

The choice feeds the next Diagnose run and **is not persisted** — it is neither
a project field nor a `config.yaml` key, and it is not a `Command`. Reopening
the panel starts at the default again.

### Meters (#809)

Under the verdict the panel draws four bars — **FIZZ**, **MUD**, **BOOM**,
**CLIP** — each reading `value / limit` for the selected genre. The tick sits
at the halfway point of the track, so the limit is always the midpoint: a bar
filling past the tick turns red, below it stays green. FIZZ, MUD and BOOM are
dimensionless power ratios; CLIP is a percentage.

Only the excess axes are metered. `Thin` and `Squash` are deficits of `MUD`
and of crest factor, so they surface as the verdict word rather than as bars of
their own.

## Per-genre calibration data (#809)

The limit table is regenerated offline by the dev-only
`openrig-tone-calibrate` binary — it **never** runs in the app or on the audio
thread, and the app reads only the committed `profiles.yaml`, so regenerating
it requires a rebuild.

Its `measure` subcommand dumps the raw per-stem measurements instead of the
aggregated table, which is what you want when checking whether a genre's
numbers are trustworthy:

```sh
cargo run -p tone-calibrate --bin openrig-tone-calibrate -- \
  measure <evaluations-root> assets/tone-profiles/genre-manifest.yaml [out.csv]
```

It writes one row per reference stem —
`song,genre,stem,mud,fizz,boom,clip,rms_dbfs,crest_db` — to `out.csv`, or to
stdout when the path is omitted. The committed dump is
`assets/tone-profiles/per-song-measurements.csv`. Methodology, the genre
labelling rules, and the caveats live in
[`development/tone-doctor-calibration.md`](development/tone-doctor-calibration.md).

## Not yet built (tracked in #791)

- Layer 1 *live* traffic light reading the output tap while playing (today the
  light updates on an explicit Diagnose run, not continuously).
- An interaction test (`i-slint-backend-testing`) driving the button → panel →
  Apply click path headlessly.
