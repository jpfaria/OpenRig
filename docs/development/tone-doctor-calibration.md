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
| `harsh` | brilliance (8–16 kHz) / body | excess | ice-pick highs |
| `boom` | low-end (40–120 Hz) / total | excess | rumble / boom |
| `clip` | fraction of rail-pinned samples | excess | clipping |
| `thin` | low-mid (160–500 Hz) / total | **deficit** | weedy, no body |
| `squash` | crest factor (peak − RMS, dB) | **deficit** | over-compressed |

Five **excess** metrics (value above the limit = bad) and two **deficit**
metrics (value below a floor = bad). `thin` and `squash` are the low tails of
`mud_ratio` and `crest_db` — a signal is `Mud` at the high end of the low-mid
band and `Thin` at the low end; dynamic at high crest and `Squash` at low crest.

Deficit floors are inherently **genre-relative** — "enough body" or "enough
dynamics" has no absolute value, only a stylistic one — so they default to
disabled and activate only under a genre's calibrated floor (the low percentile,
p10, of that genre's distribution). The excess model alone could not express
this; the classifier now scores both directions.

This tool derives **measured, per-genre limits** from real isolated-guitar
reference stems instead of guessing them.

> Scope: this is **Piece 1** — the offline calibration pipeline that produces
> the limit table. Consuming the table at runtime (a genre selector on the
> chain, `symptom()` reading the active profile) is Piece 2, a separate issue.

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
  label lives here. Seed it once, refine by ear.
- **out** — where the table is written; omit to print to stdout.

Missing stems are skipped with a note, not treated as fatal — a partial corpus
still calibrates.

## Output and confidence

```yaml
grunge:
  mud: 0.537
  fizz: 0.473        # buzzy by nature — a fixed 0.05 limit would false-flag it
  clip: 0.0
  harsh: 0.00002
  boom: 0.117
  n: 7
  confidence: trusted
metal:
  mud: 0.554
  fizz: 0.343
  clip: 0.0
  harsh: 0.00002
  boom: 0.100
  n: 2
  confidence: provisional   # too few stems — treat as a best guess
```

A genre with fewer than `MIN_CONFIDENT_SAMPLES` (6) contributing stems is marked
`provisional` so small-sample fragility stays visible rather than hidden. Fine
genre labels mean several genres will be thin until the corpus grows.

## Derived numbers (current corpus)

Calibration over 28 songs (each `lead` + `rhythm` isolated stem), p90:

| Genre | mud | fizz | harsh | boom | n | confidence |
|---|---:|---:|---:|---:|---:|---|
| blues | 0.737 | **0.016** | ~0 | 0.047 | 8 | trusted |
| clean | 0.717 | 0.205 | ~0 | — | 8 | trusted |
| grunge | 0.537 | **0.473** | ~0 | 0.117 | 7 | trusted |
| punk | 0.512 | 0.206 | ~0 | — | 6 | trusted |
| metal | 0.554 | 0.343 | ~0 | 0.100 | 2 | provisional |
| classic-rock | 0.560 | 0.230 | — | — | 4 | provisional |
| alternative-rock | 0.275 | 0.346 | — | — | 4 | provisional |
| hard-rock | 0.469 | 0.095 | — | — | 2 | provisional |
| brazilian-rock | 0.604 | 0.156 | — | — | 5 | provisional |
| jazz | 0.118 | 0.051 | — | — | 1 | provisional |
| pop-rock | 0.416 | 0.034 | — | — | 1 | provisional |

### Why this proves the premise

Look at `fizz`: **grunge 0.473** vs **blues 0.016** — a 30× spread. The old fixed
`FIZZ_RATIO_LIMIT = 0.05` would flag *every* grunge tone as fizzy (0.47 ≫ 0.05)
while treating blues as already at the edge. Per-genre limits judge each style by
its own standard: grunge is only "too fizzy" past *its* 0.47, not a universal
0.05. That is the difference between measuring physics and respecting musical
intent.

### Honest caveats

- **Small N.** Genres with 1–2 songs (jazz, metal, pop-rock) come out
  `provisional` — the number exists but is not trustworthy until the corpus
  grows. The flag keeps that in the open.
- **`harsh` ≈ 0 everywhere.** The reference stems carry little 8–16 kHz content
  (dark / band-limited masters), so the brilliance limit lands near zero. It will
  fill out with brighter stems.
- **Genre labels are subjective.** Silverchair's "Shade" is acoustic, not grunge;
  each such call is the owner's by ear — the manifest is a seed, not law.
- **Deficit auto-fix.** `thin` and `squash` are now classified and calibrated,
  but the auto-fix engine only *lowers* knobs; correcting a deficit means
  *raising* one, so the doctor reports these two but offers no one-knob fix yet.

## Constants (single source of truth)

`crates/feature-dsp/src/tone_profiles.rs`:

- `DEFAULT_PERCENTILE = 0.90`
- `MIN_CONFIDENT_SAMPLES = 6`

## Files

| Path | Role |
|---|---|
| `crates/feature-dsp/src/tone_profiles.rs` | Pure aggregation core (grouping, percentile, confidence) |
| `crates/tone-calibrate/` | Offline binary: manifest + WAV + YAML I/O |
| `assets/tone-profiles/genre-manifest.yaml` | `song -> genre` map (seed, owner-refined) |
| `assets/tone-profiles/profiles.yaml` | Generated per-genre limit table |
| `assets/tone-profiles/per-song-measurements.csv` | Raw per-song, per-stem descriptors (`measure` subcommand) |
| `docs/development/tone-calibration-per-song.html` | Self-contained chart of every song's measurements, faceted by genre |

The per-song chart shows the spread the genre summary hides — open the HTML in a
browser, or regenerate the CSV with `openrig-tone-calibrate measure`.

Design: `docs/superpowers/specs/2026-07-20-issue-809-genre-calibrated-tone-limits-design.md`.
