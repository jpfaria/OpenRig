# Genre-calibrated Tone Doctor limits (#809)

## Problem

The Tone Doctor (#791) classifies `mud` / `fizz` / `clip` against **absolute, global**
constants in `crates/feature-dsp/src/tone_descriptors.rs`
(`MUD_RATIO_LIMIT = 0.55`, `FIZZ_RATIO_LIMIT = 0.05`, `CLIP_FRACTION_LIMIT = 0.001`).
These are provisional guesses. What counts as "muddy" or "harsh" is
genre-dependent: a fuzz / high-gain tone is *meant* to be buzzy and low-mid
heavy; a clean tone is not. One fixed threshold misfires across styles.

The descriptors measure **physics** (band energy ratios, rail-pinned fraction),
not intent. No external reference publishes limits for *our* specific ratios
(`mud_ratio = low-mid / total`, `fizz_ratio = presence / body`), so the only
rigorous way to set them is to **measure them on real, genre-labeled,
isolated-guitar audio** and read the healthy distribution off our own analyzer.

## Direction (approved)

Replace guessed constants with **measured, per-genre limits** derived from a
corpus of isolated-guitar reference stems.

This ships in two separable pieces. **This spec covers Piece 1 only.**

### Piece 1 — calibration pipeline (this issue)

Offline, no audio-thread work. Static repo data out. Two units:

**A. Pure aggregation core** — new module `crates/feature-dsp/src/tone_profiles.rs`,
next to `tone_descriptors`. Consumes `ToneDescriptors`, owns no I/O.

- Input: a collection of `(genre, ToneDescriptors)` samples.
- Groups by genre; for each genre and each metric (`mud_ratio`, `fizz_ratio`,
  `clip_fraction`), derives the limit as a **high percentile (p90 default,
  tunable)** of that genre's healthy distribution — the value above which a
  render is anomalous *by that genre's own standards*.
- Emits per genre: the three limits, the sample count `n`, and a **confidence
  flag** (`Trusted` when `n >= MIN_CONFIDENT_SAMPLES`, else `Provisional`) so
  small-N fragility is visible, never hidden.
- Deterministic: same samples in → same profiles out (invariant #9).

**B. Thin binary glue** — a small tool (bin) that:
1. reads a versioned `song -> genre` manifest (YAML) — the
   `~/.openrig/evaluations/*/refs/{lead,rhythm}.wav` stems are per-song and
   *unlabeled*, so the label lives in the manifest;
2. loads each labeled stem (WAV via `hound`), runs `analyze()`;
3. feeds the `(genre, descriptors)` samples to the aggregation core;
4. writes `profiles.yaml` (`genre -> {mud, fizz, clip}` + `n` + confidence).

The core (A) is unit-tested red-first. The binary (B) is I/O glue with a small
integration test over a tiny synthetic corpus.

### Piece 2 — runtime plumbing (follow-up, out of scope here)

Manual genre selector on the chain, persisted (system vs project per ADR 0003),
with `symptom()` / `symptom_metric()` reading the active profile's limits
instead of the global constants. Separate issue.

## Data model (Piece 1)

```
enum Confidence { Trusted, Provisional }

struct GenreProfile {
    genre: String,
    mud_limit: f32,
    fizz_limit: f32,
    clip_limit: f32,
    n: usize,             // stems that contributed
    confidence: Confidence,
}
```

Aggregation entry point (pure):

```
fn calibrate(
    samples: &[(String /*genre*/, ToneDescriptors)],
    percentile: f32,     // 0.0..1.0, default 0.90
) -> Vec<GenreProfile>   // one per distinct genre, sorted by genre
```

Percentile uses linear interpolation between the two nearest ranks on the
sorted per-metric values; a single-sample genre returns that sample's value
(and is flagged `Provisional`).

## Constants

- `DEFAULT_PERCENTILE = 0.90`
- `MIN_CONFIDENT_SAMPLES = 6` — below this a genre is `Provisional`.

Both are named once in `tone_profiles.rs` (single source of truth).

## Testing (red-first)

Pure core, deterministic, no hardware, no real files:

1. grouping — mixed-genre samples land in the right buckets, one profile per
   genre, sorted.
2. percentile — a known set (e.g. 10 ascending values) yields the analytically
   correct p90 with interpolation.
3. confidence — `n < MIN_CONFIDENT_SAMPLES` → `Provisional`; `>=` → `Trusted`.
4. single sample — one stem for a genre → that value, `Provisional`, no panic.
5. determinism — same input twice → identical output.

Binary: one integration test over a synthetic 2-genre corpus writing to
`CARGO_TARGET_TMPDIR` (never the user's real files, never scratchpad).

## Out of scope

- Runtime consumption of the table (Piece 2).
- Auto-detecting genre.
- The `song -> genre` map content and final percentile value — seeded by the
  agent, refined by the owner; not a code concern.

Related: #791
