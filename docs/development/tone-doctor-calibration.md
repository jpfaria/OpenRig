# Tone Doctor — genre calibration (#809)

The Tone Doctor (#791) classifies `mud` / `fizz` / `clip` against limits. Those
limits are genre-dependent: a grunge tone is *meant* to be buzzy (high presence
energy), a blues tone is dark. A single fixed threshold misfires across styles.

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
  n: 7
  confidence: trusted
metal:
  mud: 0.554
  fizz: 0.343
  clip: 0.0
  n: 2
  confidence: provisional   # too few stems — treat as a best guess
```

A genre with fewer than `MIN_CONFIDENT_SAMPLES` (6) contributing stems is marked
`provisional` so small-sample fragility stays visible rather than hidden. Fine
genre labels mean several genres will be thin until the corpus grows.

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

Design: `docs/superpowers/specs/2026-07-20-issue-809-genre-calibrated-tone-limits-design.md`.
