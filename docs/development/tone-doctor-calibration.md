# Tone Doctor — genre calibration (#809)

The Tone Doctor (#791) classifies tonal symptoms against limits. Those limits are
genre-dependent: a grunge tone is *meant* to be buzzy (high presence energy), a
blues tone is dark. A single fixed threshold misfires across styles.

## Symptoms measured

Every symptom is a reference-free ratio the analyzer reads off a Welch power
spectrum (or the raw samples, for clip):

| Symptom | Metric | Direction | Meaning |
|---|---|---|---|
| `mud` | low-mid (160–500 Hz) / total | excess | boxy / muddy |
| `fizz` | presence (3–8 kHz) / body | excess | buzzy / fizzy |
| `boom` | low-end (40–120 Hz) / total | excess | rumble / boom |
| `clip` | fraction of rail-pinned samples | excess | clipping |
| `thin` | low-mid (160–500 Hz) / total | **deficit** | weedy, no body |
| `squash` | crest factor (peak − RMS, dB) | **deficit** | over-compressed |

Four **excess** metrics (value above the limit = bad) and two **deficit**
metrics (value below a floor = bad). `thin` and `squash` are the low tails of
`mud_ratio` and `crest_db` — a signal is `Mud` at the high end of the low-mid
band and `Thin` at the low end; dynamic at high crest and `Squash` at low crest.
(A `harsh` 8–16 kHz axis was tried and dropped: a guitar cab rolls off above
~5-6 kHz, so that band read ~0 for the whole corpus — brightness lives in `fizz`.)

Deficit floors are inherently **genre-relative** — "enough body" or "enough
dynamics" has no absolute value, only a stylistic one — so they default to
disabled and activate only under a genre's calibrated floor (the low percentile,
p10, of that genre's distribution). The excess model alone could not express
this; the classifier now scores both directions.

This tool derives **measured, per-genre limits** from real isolated-guitar
reference stems instead of guessing them.

> Scope: this document is the offline calibration pipeline that produces the
> limit table. Consuming it at runtime — the genre selector in the Tone Doctor
> panel and the classifier reading the selected profile — shipped in the same
> issue (#809) and is documented in [`../tone-doctor.md`](../tone-doctor.md).

## How it works

1. **Measure** each labeled stem with the existing reference-free analyzer
   (`feature_dsp::tone_descriptors::analyze`) → `mud_ratio`, `fizz_ratio`,
   `clip_fraction`.
2. **Group** by genre and take a high percentile (p90 default) of each metric's
   healthy distribution — the value above which a render is anomalous *by that
   genre's own standards*.
3. **Emit** `profiles.yaml`: one row per genre with its limits plus the
   evidence (`n` stems, `confidence`).

The maths lives in the pure, unit-tested core
`crates/feature-dsp/src/tone_profiles.rs`; the WAV/YAML I/O lives in the
`tone-calibrate` crate. Nothing here runs in the app or on the audio thread.

## Running

```sh
cargo run -p tone-calibrate --bin openrig-tone-calibrate -- \
  ~/.openrig/evaluations \
  assets/tone-profiles/genre-manifest.yaml \
  assets/tone-profiles/profiles.yaml \
  0.90                      # percentile, optional (defaults to 0.90)
```

- **evaluations root** — holds `<song>/refs/{lead,rhythm}.wav` isolated stems.
- **manifest** — a flat `song: genre` YAML map. The genre is *not* on disk; the
  label lives here — sourced per recording (Wikipedia / Billboard / AllMusic),
  not guessed.
- **out** — where the table is written; omit to print to stdout.

Missing stems are skipped with a note, not treated as fatal — a partial corpus
still calibrates.

## Output and confidence

```yaml
grunge:
  mud: 0.537
  fizz: 0.473        # buzzy by nature — a fixed 0.05 limit would false-flag it
  clip: 0.0
  boom: 0.117
  n: 7
  confidence: trusted
metal:
  mud: 0.554
  fizz: 0.343
  clip: 0.0
  boom: 0.100
  n: 2
  confidence: provisional   # too few stems — treat as a best guess
```

A genre with fewer than `MIN_CONFIDENT_SAMPLES` (6) contributing stems is marked
`provisional` so small-sample fragility stays visible rather than hidden. Fine
genre labels mean several genres will be thin until the corpus grows.

## Derived numbers (current corpus)

Calibration over the reference stems that exist on disk; each genre is the
**primary genre in that song's Wikipedia infobox**, fetched and verified
directly (per recording, not per band — "Enter Sandman" is heavy-metal, not
thrash). p90 for excess limits, p10 for the deficit floors (`thin`/`squash`):

| Genre | mud | fizz | boom | thin | squash | n | confidence |
|---|---:|---:|---:|---:|---:|---:|---|
| alternative-rock | 0.482 | 0.296 | 0.073 | 0.144 | 16.2 | 8 | trusted |
| grunge | 0.537 | **0.473** | 0.117 | 0.264 | 15.7 | 7 | trusted |
| blues-rock | 0.737 | **0.014** | 0.039 | 0.479 | 20.3 | 6 | trusted |
| punk-rock | 0.457 | 0.153 | 0.093 | 0.175 | 15.0 | 4 | provisional |
| rock | 0.615 | 0.314 | 0.082 | 0.208 | 17.4 | 4 | provisional |
| pop-rock | 0.572 | 0.030 | 0.042 | 0.374 | 18.6 | 3 | provisional |
| alternative-metal | 0.658 | 0.091 | 0.025 | 0.514 | 15.2 | 2 | provisional |
| art-rock | 0.526 | 0.037 | 0.020 | 0.365 | 21.6 | 2 | provisional |
| hard-rock | 0.469 | 0.095 | 0.017 | 0.227 | 20.5 | 2 | provisional |
| heavy-metal | 0.554 | 0.343 | 0.100 | 0.201 | 18.6 | 2 | provisional |
| melodic-hardcore | 0.483 | 0.246 | 0.067 | 0.124 | 18.4 | 2 | provisional |
| soft-rock | 0.720 | 0.039 | 0.006 | 0.611 | 18.9 | 2 | provisional |
| southern-rock | 0.534 | 0.285 | 0.047 | 0.276 | 20.0 | 2 | provisional |
| funk-rock · mpb | — | — | — | — | — | 1 | provisional |

### Why this proves the premise

Look at `fizz`: **grunge 0.473** vs **blues-rock 0.014** — a 30×+ spread. The old
fixed `FIZZ_RATIO_LIMIT = 0.05` would flag *every* grunge tone as fizzy (0.47 ≫
0.05) while treating blues as already at the edge. Per-genre limits judge each
style by its own standard: grunge is only "too fizzy" past *its* 0.47, not a
universal 0.05. That is the difference between measuring physics and respecting
musical intent.

### Honest caveats

- **Small N.** Genres with 1–2 songs come out `provisional` — the number exists
  but is not trustworthy until the corpus grows. The flag keeps that in the open.
- **Sourced genre, fine granularity.** Each label is grounded in a reference for
  that recording (not guessed), which yields 15 genres — several with 1–2 stems.
  Finer than the corpus can support; grouping into families (e.g. the several
  `*-rock`) would give sturdier buckets, but that is a slicing choice, not a
  licence to invent labels.
- **Live activation.** Every calibrated limit — excess and deficit alike — flows
  into the doctor through `diagnose_with_limits` / `measure_fix_with_limits`,
  and the panel's genre selector picks the row. The selection is **session-only**:
  it is not persisted to the project or `config.yaml` and is not a `Command`, so
  each Diagnose run starts from whatever the panel currently shows. With no genre
  selected the global defaults apply, which keeps the deficit floors at zero and
  therefore disabled.

## Constants (single source of truth)

`crates/feature-dsp/src/tone_profiles.rs`:

- `DEFAULT_PERCENTILE = 0.90`
- `MIN_CONFIDENT_SAMPLES = 6`

## Files

| Path | Role |
|---|---|
| `crates/feature-dsp/src/tone_profiles.rs` | Pure aggregation core (grouping, percentile, confidence) |
| `crates/tone-calibrate/` | Offline binary: manifest + WAV + YAML I/O |
| `assets/tone-profiles/genre-manifest.yaml` | `song -> genre` map (sourced per recording) |
| `assets/tone-profiles/profiles.yaml` | Generated per-genre limit table |
| `assets/tone-profiles/per-song-measurements.csv` | Raw per-song, per-stem descriptors (`measure` subcommand) |
| `docs/development/tone-calibration-per-song.html` | Self-contained chart of every song's measurements, faceted by genre |

The per-song chart shows the spread the genre summary hides — open the HTML in a
browser, or regenerate the CSV with `openrig-tone-calibrate measure`.

Design: `docs/superpowers/specs/2026-07-20-issue-809-genre-calibrated-tone-limits-design.md`.
