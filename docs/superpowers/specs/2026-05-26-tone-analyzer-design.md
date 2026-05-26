# Tone Analyzer — Audio-Driven Validation Loop for Preset Building

**Issues:** [#552](https://github.com/jpfaria/OpenRig/issues/552) (Part A — this repo) + follow-up issue in `jpfaria/OpenRig-claude` (Part B — analyzer skill).

**Status:** draft, awaiting user review.

**Authors:** João Paulo Faria + Claude.

## Problem

Building tones for real songs ("the `Cold Clocks` sound", "the `Duality` riff sound") today goes:

1. `openrig-tone-builder` skill researches gear on the web (`tonedb.co`, `groundguitar.com`, etc.).
2. It maps that gear to OpenRig blocks via `blocks-reference.md`.
3. It builds the chain on the live rig via the MCP server.
4. User plays guitar through the rig and **listens**.
5. If it sounds wrong, user tells Claude "darker", "too gainy", "missing the chorus shimmer", and the loop iterates by ear.

That last step is the weak link. The user can hear that something is off but cannot pin down which block / which parameter / by how much. The skill cannot self-correct because it has no signal: it only knows what the web said, not what was actually heard.

We need to close the loop with **measurement**: compare what the chain produces against a reference recording of the target tone, derive a structured diff, and feed that diff back into the preset builder so it can adjust deterministically.

## Non-problems (out of scope)

- Naming the exact pedal/amp from audio alone (impossible at useful confidence — the `openrig-tone-builder` web research stays the source of names).
- Real-time analysis during live playing.
- Tempo-mapped automation, MIDI replay, multi-take rendering.
- A web UI for the analyzer — it is a CLI/skill, output is JSON + spectrograms.

## High-level architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  PART A — this repo (OpenRig)                                    │
│  Issue #552: feat(cli) offline render mode                       │
│                                                                  │
│  openrig --render --project P.openrig                            │
│                  --input DI.wav                                  │
│                  --output wet.wav                                │
│                                                                  │
│  Headless, no audio device, no GUI. Same engine.process_block()  │
│  as live mode → deterministic, byte-identical given same input.  │
└──────────────────────────────────────────────────────────────────┘
                              │ ships as openrig binary feature
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│  PART B — separate repo (OpenRig-claude plugin)                  │
│  New skill: openrig-tone-analyzer                                │
│  Path: skills/openrig-tone-analyzer/                             │
│                                                                  │
│  Pure functions, no MCP, no orchestration:                       │
│    • analyze  ref.wav            → fingerprint.json + spec.png   │
│    • compare  ref.wav  wet.wav   → diff.json + ab_spec.png       │
│                                                                  │
│  Stack: ffmpeg (always) + Python venv (librosa, numpy, scipy)    │
│         + spectrogram PNGs that Claude reads as visual evidence  │
└──────────────────────────────────────────────────────────────────┘
                              │ JSON contracts
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│  openrig-tone-builder (already exists, in OpenRig-claude)        │
│  Becomes orchestrator of the validation loop:                    │
│                                                                  │
│   1. analyzer.analyze(ref.wav)     → fingerprint                 │
│   2. web research + fingerprint    → initial chain via MCP       │
│   3. openrig --render              → wet.wav                     │
│   4. analyzer.compare(ref, wet)    → diff                        │
│   5. apply diff → MCP param tweaks                               │
│   6. goto 3, up to MAX_ITERATIONS                                │
│   7. if not converged, hand back to user with the diff           │
└──────────────────────────────────────────────────────────────────┘
```

### Why this shape

- **Analyzer is a pure function.** It does not know about MCP, render binaries, or the chain. It just takes WAVs and emits JSON. This makes it independently testable, replaceable, and reusable (e.g. future preset diffing in CI, golden-tone regression tests).
- **Render lives in the main repo.** Offline render is a feature of the engine + CLI, not of the plugin. Other consumers (golden tests, CI, future tools) benefit equally.
- **`tone-builder` owns the loop.** Orchestration is policy (when to stop, how to translate a diff into MCP calls, which block to blame for a given EQ delta). Policy belongs next to the thing it controls; it does not belong in a measurement tool.
- **No new long-running daemon.** All three pieces are short-lived processes glued by JSON files and the existing MCP server.

## Part A — Offline render mode (this issue, #552)

Detailed scope, CLI surface, acceptance criteria, and file layout are in the issue body. Summary here only:

- New crate `crates/adapter-render` with binary path wired into `openrig --render`.
- Loads `.openrig` project, processes input WAV through the engine, writes output WAV.
- Headless: no `cpal`, no Slint, no MCP, no MIDI.
- Deterministic: same input → byte-identical output.
- Realtime invariants preserved inside `engine.process_block()`.
- TDD red-first; golden-WAV integration tests with pinned fixtures.
- Docs updated: `docs/cli.md`, `docs/architecture.md`, `docs/testing.md`, `CHANGELOG.md`.

This is **the prerequisite**. Until #552 ships, Part B has no way to obtain `wet.wav` and the loop cannot close.

## Part B — `openrig-tone-analyzer` skill (follow-up, OpenRig-claude repo)

### Repo layout

```
~/.claude/plugins/marketplaces/openrig/   # local clone of jpfaria/OpenRig-claude
└── skills/
    └── openrig-tone-analyzer/
        ├── SKILL.md                  # invocation contract + workflow
        ├── scripts/
        │   ├── analyze.py            # IN: wav | OUT: fingerprint.json + spec.png
        │   ├── compare.py            # IN: ref.wav wet.wav | OUT: diff.json + ab_spec.png
        │   └── _common.py            # shared spectral helpers
        ├── requirements.txt          # librosa, numpy, scipy, soundfile, matplotlib
        ├── bootstrap.sh              # creates .venv on first run, idempotent
        └── README.md                 # human-facing notes
```

### Modes

The skill exposes two operations, each callable as a Bash one-liner from `SKILL.md`:

#### `analyze <input.wav> [--out-dir DIR]`

- Decode input via `soundfile` (or `ffmpeg` fallback for non-PCM).
- Convert to mono float32 at a fixed analysis sample rate (e.g. 44_100 Hz) for descriptors that benefit from it; keep stereo separately for stereo-aware features.
- Emit `fingerprint.json` (schema below).
- Emit `spec.png` — Mel spectrogram, log-frequency Y, dB color scale, 4-second window centered on the loudest segment.

#### `compare <ref.wav> <wet.wav> [--out-dir DIR]`

- Run `analyze` internally on both WAVs (cached on hash to avoid recomputing).
- Time-align the two signals via cross-correlation on the first ~2 s — needed because `wet.wav` includes a `--tail-ms` window that `ref.wav` does not, plus possibly different latencies.
- Emit `diff.json` (schema below) with **directional** deltas (wet vs ref).
- Emit `ab_spec.png` — side-by-side spectrograms (reference left, rendered right), shared color scale.

### JSON contracts

#### `fingerprint.json`

```json
{
  "schema_version": 1,
  "source": {
    "path": "/abs/path/to/ref.wav",
    "sha256": "deadbeef...",
    "sample_rate_hz": 48000,
    "channels": 2,
    "duration_s": 18.42
  },
  "loudness": {
    "rms_db": -14.2,
    "peak_db": -0.7,
    "crest_factor_db": 13.5,
    "lufs_integrated": -10.1
  },
  "spectrum": {
    "bands_hz": [80, 160, 320, 640, 1280, 2560, 5120, 10240],
    "band_energy_db": [-22.1, -18.3, -14.8, -12.0, -11.2, -13.5, -17.9, -25.4],
    "spectral_centroid_hz": 1820.0,
    "spectral_rolloff_hz_85pct": 4200.0,
    "spectral_flatness": 0.18
  },
  "distortion": {
    "thd_estimate_pct": 18.4,
    "odd_to_even_harmonic_ratio_db": 6.8,
    "gain_character": "high_gain",
    "gain_character_confidence": 0.82
  },
  "time_fx": {
    "reverb_rt60_s": 1.4,
    "reverb_rt60_confidence": 0.71,
    "delay_present": true,
    "delay_time_ms_estimate": 380,
    "delay_feedback_estimate_pct": 24,
    "modulation_present": false,
    "modulation_rate_hz": null,
    "modulation_depth_estimate": null
  },
  "stereo": {
    "is_stereo": true,
    "ms_balance_ratio": 0.82,
    "lr_correlation": 0.91
  }
}
```

**Fields are advisory, not authoritative.** Each metric is a best-effort estimate from a short audio clip; the analyzer documents confidence where it can. Consumers (`tone-builder`) treat them as evidence, not ground truth.

#### `diff.json`

```json
{
  "schema_version": 1,
  "reference": "fingerprint.json sha256 ...",
  "rendered":  "fingerprint.json sha256 ...",
  "match_score": 0.71,
  "delta": {
    "rms_db":         { "wet_minus_ref": -2.1, "verdict": "wet quieter" },
    "spectral_centroid_hz": { "wet_minus_ref": -340, "verdict": "wet darker" },
    "band_energy_db": [
      { "band_hz":   80, "delta_db": +1.2 },
      { "band_hz":  160, "delta_db": +0.8 },
      { "band_hz":  320, "delta_db": -0.3 },
      { "band_hz":  640, "delta_db": -1.4 },
      { "band_hz": 1280, "delta_db": -2.9 },
      { "band_hz": 2560, "delta_db": -3.8 },
      { "band_hz": 5120, "delta_db": -2.1 },
      { "band_hz":10240, "delta_db": +0.4 }
    ],
    "thd_estimate_pct": { "wet_minus_ref": -4.2, "verdict": "wet less distorted" },
    "reverb_rt60_s":    { "wet_minus_ref": -0.6, "verdict": "wet shorter tail" },
    "delay_present":    { "ref": true, "wet": false, "verdict": "wet missing delay" },
    "modulation_present": { "ref": false, "wet": false, "verdict": "ok" }
  },
  "recommendations": [
    { "priority": 1, "target": "amp",     "action": "increase gain by ~15%",
      "rationale": "THD lower by 4.2 pts and 2-5 kHz energy lower by 3 dB — wet is cleaner than ref" },
    { "priority": 2, "target": "eq_eight_band_parametric",
      "action": "boost 2 kHz band by +3 dB (Q ~1.0)",
      "rationale": "mid-range energy deficit at 1.28–2.56 kHz" },
    { "priority": 3, "target": "delay",   "action": "enable delay block, time ~380 ms, feedback ~25%, mix ~12%",
      "rationale": "reference shows periodic echo at 380 ms; wet has none" },
    { "priority": 4, "target": "reverb",  "action": "increase room_size to extend tail by ~0.6 s",
      "rationale": "RT60 deficit of 0.6 s" }
  ],
  "converged": false,
  "convergence_threshold": { "match_score_min": 0.85, "max_abs_band_delta_db": 2.0 }
}
```

`match_score` is a single 0..1 number combining band-energy RMS distance, centroid delta, THD delta, and reverb tail delta, each weighted by perceptual relevance (mids weigh more than sub-bass; THD matters more than centroid for distortion presets). Exact weighting documented in `compare.py` and pinned in tests.

`recommendations` is the **only** field `tone-builder` is required to consume. The rest is observability. Recommendations are ordered by priority and each names a `target` block kind (`amp`, `eq_eight_band_parametric`, `delay`, `reverb`, …) so the builder can look up the corresponding block id in the live project and dispatch the MCP call.

### Tool stack

- **`ffmpeg`** — required system dep. Used for decoding non-PCM inputs, generating fallback spectrograms via `showspectrumpic` if Python is unavailable.
- **`python3` ≥ 3.10** — required system dep. Skill bootstraps a local venv in `skills/openrig-tone-analyzer/.venv/` on first run via `bootstrap.sh`. Venv is gitignored.
- **Python deps** (`requirements.txt`):
  - `numpy`, `scipy` — spectral math
  - `librosa` — MFCC, spectral centroid/rolloff/flatness, onset detection, harmonic separation
  - `soundfile` — WAV I/O
  - `matplotlib` — spectrograms
  - `pyloudnorm` — LUFS measurement
- **No** Rust dep, **no** OpenRig dep. The skill is pure-Python + ffmpeg.

### `SKILL.md` shape (preview)

```
---
name: openrig-tone-analyzer
description: Use when the user asks to analyze a guitar audio file ("analisa
  esse áudio", "compara o som que saiu com a referência", "validar o timbre
  X"). Runs as a pure function: in = wav files, out = JSON + spectrogram PNGs.
  Does NOT call MCP, does NOT modify the rig. The openrig-tone-builder skill
  orchestrates the validation loop using this skill's outputs.
---

# Skill workflow

1. Confirm ffmpeg + python3 are installed (`which ffmpeg && python3 --version`).
2. On first run, execute `./bootstrap.sh` in the skill dir to set up the venv.
3. Pick mode:
   - User gave 1 WAV → `analyze <wav>` → present fingerprint.json + spec.png
     to the user (read PNG as image evidence; summarize key fields in chat).
   - User gave 2 WAVs (ref + wet) → `compare <ref> <wet>` → present
     diff.json + ab_spec.png; surface top recommendations in chat (one
     sentence each).
4. Output files go to `/tmp/openrig-analyzer/<timestamp>/`.
5. Never write to the OpenRig project, never call MCP tools, never invoke
   `openrig --render`. Those are the orchestrator's job.

# Anti-patterns
- Pretending to identify exact amp/pedal models from audio — name claims
  come from openrig-tone-builder's web research, not from analysis.
- Claiming match_score reflects "how it sounds to a human" — it is a
  weighted technical distance. Document this in the chat reply.
- Persisting analysis output anywhere other than the configured out dir.
```

### Acceptance criteria for Part B

(Implemented in the OpenRig-claude repo, in its own PR.)

- [ ] `bootstrap.sh` is idempotent: first run creates venv + installs deps; subsequent runs are no-ops within ~1 s.
- [ ] `analyze` on a 5-second test WAV (committed as fixture) produces a `fingerprint.json` whose every field is non-null, non-NaN, and within expected ranges.
- [ ] `analyze` on the same WAV twice produces byte-identical `fingerprint.json` (determinism — seed any randomness; round floats consistently).
- [ ] `compare` on identical WAVs produces `match_score >= 0.99` and an empty `recommendations` array.
- [ ] `compare` on a known-different pair (clean vs distorted fixture) produces `match_score < 0.5` and a `gain_character`-related recommendation.
- [ ] `compare` correctly time-aligns when `wet.wav` has the `--tail-ms` window from `openrig --render` (test fixture includes a 2 s silent tail).
- [ ] `compare` flags missing `delay` and missing `reverb` when the wet chain has those blocks disabled vs a ref recording that exhibits them.
- [ ] No network calls during analyze/compare (offline by design).
- [ ] Spectrogram PNGs render at ≥ 1024×512 with axis labels, color bar, and frequency in log scale.
- [ ] CI on the OpenRig-claude repo runs the fixture tests in < 60 s on macOS + Linux.

## Loop policy (lives in `openrig-tone-builder`, not in analyzer)

- `MAX_ITERATIONS = 5` per tone-building session. Empirical default; tunable.
- Convergence: `match_score >= 0.85` **and** `max(abs(band_delta_db)) <= 2.0`.
- Per iteration: apply at most the top 2 recommendations from `diff.json` (avoid thrashing — one fix may resolve multiple symptoms).
- After non-convergence at `MAX_ITERATIONS`: present the last `diff.json` + `ab_spec.png` to the user with a one-line summary and ask whether to (a) continue another N iterations, (b) accept current state, or (c) revert to iteration K's chain.
- Iteration audit trail: each iteration writes `iter_<N>_diff.json` and `iter_<N>_wet.wav` to a session dir under `/tmp/openrig-tone-session/<song>/`. Persisted at least until the chat session ends; cleaned up by the OS or by a final user command.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Time alignment fails on highly distorted signals (cross-correlation noisy). | Fall back to onset-based alignment (`librosa.onset.onset_detect`) — first onset in ref vs wet. If both fail, report alignment uncertainty in `diff.json.delta.alignment_confidence` and skip time-domain metrics. |
| `match_score` weighting is wrong (matches numerically but sounds different, or vice versa). | The weighting is pinned in tests with curated fixture pairs annotated "should match" / "should not match". Tweaking weights is allowed only if all pinned cases still pass. |
| `tone-builder` over-corrects, oscillates between fixes. | Loop policy caps at 2 changes/iteration and 5 iterations. If `match_score` regresses 2 iterations in a row, abort and report. |
| Python venv setup fails on user's machine (system Python issues). | `bootstrap.sh` checks `python3 --version` first, prints a clear "install python ≥ 3.10" if absent. Falls back to `ffmpeg`-only mode for spectrograms (no fingerprint.json — degraded). |
| Determinism breaks across librosa versions. | `requirements.txt` pins exact versions. Determinism test runs in CI on the OpenRig-claude repo. |
| Issue #552 takes longer than expected → Part B starts producing reports against a fake "wet.wav" generated by hand. | Until #552 ships, `analyze` works fine standalone (user can validate the analyzer against the reference alone). `compare` simply requires the wet WAV to exist — it does not care how it was produced. So Part B can start integration-testing once any wet WAV is available, even a manually exported one from the live rig. |

## Sequencing

1. **#552 (this repo).** Issue is open. Spec for it is the issue body itself. Implementation TDD-red-first, then PR to `develop`.
2. **Part B issue in OpenRig-claude.** Open after #552 is merged (or once `openrig --render` is reliably available on a feature branch the user can build locally). Reference this spec from the new issue.
3. **`tone-builder` orchestrator update.** A small PR in OpenRig-claude that teaches `openrig-tone-builder` to invoke analyzer + render in the loop. May be bundled with the Part B PR or split.

## Out of scope (revisit later)

- Real-time analysis during live playing (would need a live-audio MCP resource feeding spectral data; large project, separate spec).
- Multi-take aggregation (analyze 3 different takes of the same song, build an averaged fingerprint).
- Cross-song fingerprint library (build a corpus, search by similarity to find "songs that sound like this preset").
- Genre/era classification heuristics.
- A graphical UI for the analyzer — JSON + PNGs are enough; Claude reads PNGs as images during chat.

## Open questions for user review

- [ ] Confirm convergence threshold: `match_score >= 0.85` and `max band delta <= 2 dB`. If too strict, loop will rarely converge; if too loose, output won't feel "right".
- [ ] Confirm `MAX_ITERATIONS = 5`. Higher = more wall time per tone; lower = more frequent user hand-back.
- [ ] Confirm Python venv approach inside the skill dir (vs system Python with `pip install --user`, vs `uv` for fast bootstrap). Venv is the most isolated and least surprising default.
- [ ] Spectrogram orientation in `ab_spec.png`: side-by-side (chosen) vs stacked vertically (more like a DAW EQ view). Side-by-side wins for direct A/B comparison but uses more horizontal space.
