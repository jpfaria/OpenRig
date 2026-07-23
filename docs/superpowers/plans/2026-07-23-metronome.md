# Metronome Implementation Plan (issue #14)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A global built-in metronome — tempo, time signature, accent, subdivisions, count-in, tap tempo, visual beat indicator — that plays through its own dedicated output stream and never touches the guitar audio path.

**Architecture:** Pure DSP generator in `feature-dsp` (sample-accurate beat phase, synthesized click, zero allocation in `render`), driven by a dedicated cpal output stream in `infra-cpal` modelled on the DI's own isolated stream (#808). State changes go through `Command` so MCP/gRPC/MIDI reach it by the same door as the GUI. UI follows the Tuner quartet (`*_wiring.rs`, `*_session.rs`, `*_close.rs`, `pages/*_window.slint`).

**Tech Stack:** Rust, cpal, Slint, `arc_swap`, atomics.

## Global Constraints

- Design of record: issue #14 comment `5063758859`. Do not re-decide anything settled there.
- **Isolation (CLAUDE.md invariant #4):** zero new lines in `process_output_f32`, `process_output_f32_mixed`, `runtime_process_segment`, `runtime.rs` or the output limiter path. The metronome mixes only in the backend.
- **Invariant #8:** no allocation, lock, syscall or I/O in `render` or in the audio callback.
- **No hardcoded sample rate** — `crates/engine/src/hardcoded_sample_rate_audit_tests.rs` fails the build otherwise.
- **TDD red-first:** every behaviour starts with a test that is observed FAILING on an assertion (not a compile error). No test is written after its production code.
- Zero warnings (`cargo build` clean), zero clippy violations.
- Repo content in English: code, comments, docs, commits.
- Tests live beside the module as `<module>_tests.rs` with `#[cfg(test)] #[path = "..."] mod tests;` — the established convention.
- Icons are SVG via `@image-url` + colorize. Never a glyph (tofu on Orange Pi).
- New user-facing strings: run `extract-translations.sh` and fill all 9 locales in the same PR.

---

### Task 1: Click generator (pure DSP)

**Files:**
- Create: `crates/feature-dsp/src/metronome.rs`
- Create: `crates/feature-dsp/src/metronome_tests.rs`
- Modify: `crates/feature-dsp/src/lib.rs` (add `pub mod metronome;`)

**Interfaces — Produces:**

```rust
pub const BPM_MIN: f32 = 30.0;
pub const BPM_MAX: f32 = 300.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Subdivision { #[default] Off, Eighths, Triplets, Sixteenths }
impl Subdivision { pub fn ticks_per_beat(self) -> u32; }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Timbre { #[default] Click, Wood, Beep }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetronomeSettings {
    pub bpm: f32,
    pub beats_per_bar: u32,
    pub subdivision: Subdivision,
    pub timbre: Timbre,
    pub volume: f32,   // 0.0..=1.0
    pub count_in: bool,
}
impl Default for MetronomeSettings; // 120 bpm, 4 beats, Off, Click, 0.7, false

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeatPosition { pub bar: u32, pub beat: u32, pub tick: u32, pub counting_in: bool }

pub struct MetronomeGenerator;
impl MetronomeGenerator {
    pub fn new(sample_rate: f32, settings: MetronomeSettings) -> Self;
    pub fn apply(&mut self, settings: MetronomeSettings);  // live, phase-preserving
    pub fn restart(&mut self);                             // bar 1, runs count-in if enabled
    pub fn render(&mut self, out: &mut [f32]);             // MONO, overwrites, zero-alloc
    pub fn position(&self) -> BeatPosition;
}
```

**Design notes for the implementer:**

Beat phase is an `f64` accumulator advanced **per sample**, never per buffer: `phase += 1.0 / samples_per_tick` where `samples_per_tick = sample_rate * 60.0 / (bpm * ticks_per_beat)`. A tick fires on the sample where `phase >= 1.0`, and `phase -= 1.0` keeps the fractional remainder — this is what makes onsets sample-accurate and drift-free regardless of callback size. Start with `phase = 1.0` so the first sample of a restart is a downbeat.

`apply` recomputes `samples_per_tick` but **keeps `phase`** — that is why a live BPM change produces no discontinuity. It must not reset `tick_index` unless `beats_per_bar` or `subdivision` changed the bar structure.

Two `ClickVoice` slots, round-robin, so a retrigger never truncates the previous click audibly. Each voice is a sine with an exponential decay envelope; `next_sample` returns exactly `0.0` once the envelope drops below `1e-5` so idle rendering costs nothing. All state is plain floats — nothing is allocated in `render`.

Timbre constants (base = plain beat; downbeat is the accent; subdivision reuses the base frequency at −8 dB ≈ `0.398`; count-in gets its own pitch so it is unmistakable):

| Timbre | beat Hz | downbeat Hz | count-in Hz | decay |
|---|---|---|---|---|
| Click | 1000 | 1600 | 2000 | 25 ms |
| Wood | 800 | 1200 | 1600 | 40 ms |
| Beep | 880 | 1320 | 1760 | 80 ms |

Count-in: when enabled, `restart` plays exactly one bar of count-in ticks (`counting_in: true` in `position()`), then continues normally. Subdivisions are suppressed during the count-in bar.

- [ ] **Step 1: Write the failing timing test**

```rust
// crates/feature-dsp/src/metronome_tests.rs
use super::*;

/// Sample indices where the rendered signal starts a click.
fn onsets(buf: &[f32]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut prev = 0.0f32;
    for (i, &s) in buf.iter().enumerate() {
        if prev.abs() < 1e-6 && s.abs() > 1e-6 {
            out.push(i);
        }
        prev = s;
    }
    out
}

#[test]
fn onsets_land_on_the_beat_at_any_rate() {
    for &rate in &[44_100.0f32, 48_000.0] {
        let settings = MetronomeSettings { bpm: 120.0, ..Default::default() };
        let mut gen = MetronomeGenerator::new(rate, settings);
        let mut buf = vec![0.0f32; (rate * 4.0) as usize]; // 4 s = 8 beats at 120 bpm
        gen.render(&mut buf);

        let expected: Vec<usize> = (0..8).map(|b| (b as f32 * rate * 0.5) as usize).collect();
        let got = onsets(&buf);
        assert_eq!(got.len(), expected.len(), "beat count at {rate} Hz");
        for (g, e) in got.iter().zip(&expected) {
            assert!(
                (*g as i64 - *e as i64).abs() <= 1,
                "onset {g} should be within 1 sample of {e} at {rate} Hz"
            );
        }
    }
}
```

- [ ] **Step 2: Run it and watch it FAIL on the assertion**

Run: `cargo test -p feature-dsp metronome -- --nocapture`
Expected: fails. Record the FAILED line in the issue comment — a compile error does not count as the behavioural red.

- [ ] **Step 3: Implement `metronome.rs` until it passes**

- [ ] **Step 4: Add the remaining tests, each red before its code**

- `no_drift_over_ten_minutes` — render 10 min at 44.1 kHz; last onset within 1 sample of the ideal position.
- `callback_size_does_not_move_onsets` — render the same 4 s in blocks of 64, 128, 512, 480 and 1023 frames; onset lists are identical to the single-block render. 480 and 1023 matter because they are not divisors of the beat period.
- `downbeat_is_louder_than_beat` — peak of the first click exceeds the second in a 4/4 bar.
- `subdivision_is_eight_db_below_the_beat` — with `Subdivision::Eighths`, the off-beat click peak is within 5 % of `0.398 * beat_peak`.
- `beats_per_bar_drives_the_accent` — in 3/4 the accent recurs every 3 clicks.
- `count_in_adds_exactly_one_bar` — with `count_in: true`, `position().counting_in` is true for exactly `beats_per_bar` ticks after `restart`.
- `live_bpm_change_has_no_discontinuity` — call `apply` with a new BPM mid-render; no sample-to-sample jump above 0.5 anywhere in the output.
- `volume_scales_the_output` — peak scales linearly with `volume`.
- `render_is_silent_between_clicks` — the tail between onsets is exactly `0.0`.

- [ ] **Step 5: Commit**

```bash
git add crates/feature-dsp/src/metronome.rs crates/feature-dsp/src/metronome_tests.rs crates/feature-dsp/src/lib.rs
git commit -m "feat(#14): sample-accurate metronome click generator"
```

---

### Task 2: Shared RT state

**Files:**
- Create: `crates/engine/src/metronome_state.rs`
- Create: `crates/engine/src/metronome_state_tests.rs`
- Modify: `crates/engine/src/lib.rs`

**Interfaces — Consumes:** `feature_dsp::metronome::{MetronomeSettings, BeatPosition}` (Task 1).

**Produces:**

```rust
/// Lock-free bridge between the control side (GUI/dispatcher) and the
/// metronome's audio callback. Every field is an atomic — the callback
/// never locks (invariant #8).
pub struct MetronomeShared { /* atomics */ }
pub type MetronomeCell = Arc<MetronomeShared>;

impl MetronomeShared {
    pub fn new(settings: MetronomeSettings) -> Self;
    pub fn set_enabled(&self, on: bool);
    pub fn enabled(&self) -> bool;
    pub fn set_settings(&self, s: MetronomeSettings);
    pub fn settings(&self) -> MetronomeSettings;
    /// Bumped by any change the renderer must pick up; the callback compares
    /// it against its own copy instead of re-reading every field per buffer.
    pub fn generation(&self) -> u64;
    pub fn request_restart(&self);
    pub fn take_restart(&self) -> bool;
    /// Published by the callback, read by the UI timer.
    pub fn publish_position(&self, pos: BeatPosition);
    pub fn position(&self) -> BeatPosition;
}
```

Pack `BeatPosition` into a single `AtomicU64` so the UI never reads a torn value (bar:u32 is truncated to 16 bits — a practice session never reaches 65 535 bars, and it wraps harmlessly).

Tests: `set_settings_bumps_generation`, `position_round_trips_through_the_atomic`, `take_restart_is_one_shot`, `enabled_defaults_to_false`.

- [ ] **Step 1–4:** red test → implement → green → commit.

```bash
git commit -m "feat(#14): lock-free shared state for the metronome stream"
```

---

### Task 3: Dedicated output stream

**Files:**
- Create: `crates/infra-cpal/src/metronome_stream.rs`
- Create: `crates/infra-cpal/src/metronome_stream_tests.rs`
- Modify: `crates/infra-cpal/src/lib.rs`, `crates/infra-cpal/src/controller.rs`

**Interfaces — Consumes:** `MetronomeCell` (Task 2), `MetronomeGenerator` (Task 1).

**Produces:**

```rust
impl ProjectRuntimeController {
    /// Open the metronome's OWN output stream on `device_id`. Never shares a
    /// chain stream — the backend sums (invariant #4).
    pub fn start_metronome(&self, device_id: &str) -> Result<()>;
    pub fn stop_metronome(&self);
    pub fn metronome_active(&self) -> bool;
    pub fn metronome_shared(&self) -> MetronomeCell;
}
```

Model it on `di_stream.rs::build_di_output_stream` — resolve the device, take `default_output_config()`, build the stream, `play()`. Differences: no chain, no `ChainId`, no SPSC ring and no worker thread. The click is generated **procedurally inside the callback** (that is why the DI's ring machinery is unnecessary here): the callback owns a `MetronomeGenerator`, renders one mono scratch buffer (pre-allocated at stream-build time, resized only on growth like the DI's `mix_scratch`) and writes it to every channel.

The callback re-reads `shared.generation()` once per buffer and calls `generator.apply(...)` only when it changed. `sample_rate` comes from the resolved device config — never a constant.

**Guard the JACK build** exactly like `build_di_output_stream`: `#[cfg(all(target_os = "linux", feature = "jack"))]` returns a no-op so Orange Pi behaviour is unchanged.

Tests are the offline-testable parts (no hardware): the callback body is extracted into `fn fill_metronome_buffer(generator, scratch, out, channels)` so it can be tested directly — `writes_the_same_click_to_every_channel`, `scratch_is_reused_across_calls`, `silent_when_disabled`.

- [ ] **Step 1–4:** red → implement → green → commit.

```bash
git commit -m "feat(#14): dedicated cpal output stream for the metronome"
```

---

### Task 4: Commands, events, dispatcher, selection state

**Files:**
- Modify: `crates/application/src/command.rs`, `event.rs`, `selection_state.rs`, `local_dispatcher.rs`
- Create: `crates/application/src/local_dispatcher_metronome.rs`, `local_dispatcher_metronome_tests.rs`

Follow `local_dispatcher_diagnostic.rs` exactly — it is the canonical 39-line file-per-feature handler.

**Commands:** `SetMetronomeEnabled { enabled }`, `SetMetronomeBpm { bpm }`, `SetMetronomeTimeSignature { beats_per_bar }`, `SetMetronomeSubdivision { subdivision }`, `SetMetronomeVolume { volume }`, `SetMetronomeTimbre { timbre }`, `SetMetronomeCountIn { enabled }`, `SetMetronomeOutput { device_id }`, `MetronomeTap`.

**Events:** one `…Changed` per command, mirroring `TunerEnabledChanged`.

`SelectionState` gains `metronome_enabled: bool` so the MIDI slot can flip it, mirroring `tuner_enabled`.

BPM is clamped to `BPM_MIN..=BPM_MAX` and volume to `0.0..=1.0` **in the dispatcher** — an out-of-range MCP call must not reach the audio thread.

Tests: `bpm_is_clamped_to_the_supported_range`, `volume_is_clamped`, `enabling_mirrors_into_selection_state`, `each_command_emits_its_event`. The existing `crates/application/tests/issue_489_command_schema_completeness.rs` must stay green.

```bash
git commit -m "feat(#14): metronome commands, events and dispatcher handler"
```

---

### Task 5: System config persistence

**Files:**
- Modify: `crates/infra-filesystem/src/app_config_io.rs`, `crates/application/src/app_config_persist.rs`
- Create: `crates/application/tests/issue_14_metronome_config_persistence.rs`

Per ADR 0003 this is **system** config (`config.yaml`) — a practice tempo does not travel in `.openrig`.

Persisted: `bpm`, `beats_per_bar`, `subdivision`, `timbre`, `volume`, `count_in`, `output_device`.
**Not persisted:** `enabled` — the app always starts with the metronome off.

Tests: `settings_survive_a_reload`, `enabled_is_never_written`, `a_whole_config_resave_preserves_metronome_settings` (the trap #607/#627 already hit twice), `a_missing_metronome_section_loads_defaults`.

Use `CARGO_TARGET_TMPDIR`/`tempfile` — never the user's real config (#701/#731).

```bash
git commit -m "feat(#14): persist metronome settings in system config"
```

---

### Task 6: MIDI slots

**Files:**
- Modify: `crates/adapter-midi/src/slots.rs`
- Create/Modify: the matching `*_tests.rs`

Add `toggle_metronome` (reads `SelectionState::metronome_enabled`, dispatches the negation — same shape as `toggle_tuner`) and `metronome_tap` (dispatches `Command::MetronomeTap`).

```bash
git commit -m "feat(#14): MIDI slots for metronome toggle and tap tempo"
```

---

### Task 7: UI

**Files:**
- Create: `crates/adapter-gui/ui/pages/metronome_window.slint`, `crates/adapter-gui/ui/assets/metronome.svg`
- Modify: `crates/adapter-gui/ui/pages/pages.slint`, `ui/app-window.slint`, `ui/models.slint`, `ui/desktop_main.slint`, `ui/touch_main.slint`, `ui/pages/project_chains.slint`

**REQUIRED before touching any `.slint`:** invoke `claude-plugin:ux-ui`, `slint-best-practices`. Render with `tools/slint-render` and inspect the PNG before claiming anything is done.

Mirror `tuner_window.slint`: define `MetronomeWindow` (standalone) **and** `MetronomePanel` (reusable inline for fullscreen/touch).

Controls: BPM (large readout + −/+ + drag), tap button, time signature select, subdivision select, timbre select, volume, count-in toggle, output device select, power foot-switch. Reuse the existing shared select component (#754) — never a new per-feature dropdown.

Beat indicator: `beats_per_bar` dots, the current one lit, the downbeat visually distinct.

Top bar: the icons in `project_chains.slint:140-175` are absolutely positioned (`parent.width - 240/200/160/120/80px`). Adding one requires **shifting the whole row**, not squeezing in. Verify with a render at several window widths.

**A render only proves layout.** Prove the controls actually work with an `i-slint-backend-testing` interaction test before pushing (memory: 4 failed UI pushes on #749/#761). `PopupWindow` content does not receive clicks — use a root-level overlay.

```bash
git commit -m "feat(#14): metronome window, panel and top-bar entry"
```

---

### Task 8: GUI wiring

**Files:**
- Create: `crates/adapter-gui/src/metronome_wiring.rs`, `metronome_session.rs`, `metronome_close.rs` (+ tests)
- Modify: `crates/adapter-gui/src/lib.rs` and wherever `wire_tuner` is called

Same quartet as the Tuner. The session owns the tap-tempo history and a 33 ms `slint::Timer` that reads `MetronomeShared::position()` and updates the indicator. It reads **phase**, not a queue, so a slow UI frame cannot lose or double a beat.

Tap tempo lives in a pure, testable function:

```rust
/// Average of the last taps, ignoring gaps above 2 s (a new count-off).
/// `None` until there are at least two usable taps.
pub fn tap_bpm(intervals: &[Duration]) -> Option<f32>;
```

Tests: `two_taps_give_the_interval_bpm`, `a_gap_above_two_seconds_restarts_the_count`, `only_the_last_four_intervals_count`, `result_is_clamped_to_the_bpm_range`.

```bash
git commit -m "feat(#14): wire the metronome UI to the engine"
```

---

### Task 9: Translations and docs

**Files:**
- Modify: all 9 locale files, `docs/screens.md`, `docs/audio-config.md`, `README.md` + `README.pt-BR.md` + `README.es-ES.md`

Run `extract-translations.sh`, fill every locale (English is the reference). Document the metronome screen in `docs/screens.md` and the dedicated-stream behaviour in `docs/audio-config.md`. Update the roadmap line in all three READMEs.

```bash
git commit -m "docs(#14): document the metronome and refresh translations"
```

---

## Verification before claiming done

- `cargo build --workspace` — zero warnings.
- `cargo clippy --workspace --all-targets` — clean.
- `cargo test --workspace` — green.
- `tools/slint-render` PNG inspected for the window, the panel and the top bar at several widths.
- Interaction test proving the controls respond.
- Guitar-path invariants untouched: `git diff origin/develop --stat` shows **no** change to `runtime.rs`, `runtime_process_segment.rs` or the limiter; `volume_invariants_tests.rs` green.
