# Score Reader — Phase 1 (read and draw) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open a Guitar Pro file in OpenRig and see it rendered as tablature and standard notation, with track selection and scrolling. No audio.

**Architecture:** Three layers with hard boundaries. `score` owns the domain model and the file readers. `score-layout` turns a `Song` into positioned drawing primitives and knows nothing about any UI toolkit. `adapter-gui` draws those primitives with Slint `Path`/`Text`/`Rectangle` and computes no layout of its own. State changes go through `Command` as every OpenRig feature does (#127).

**Tech Stack:** Rust, `nom` 8 (binary parsing), `zip` + `roxmltree` (GP7/GP8), `smufl` (glyph metrics), Bravura OTF (SIL OFL), Slint 1.16.

## Global Constraints

- **Language:** all repo content in English — code comments, doc files, commit messages, issue comments. Chat with the owner is pt-BR.
- **Zero warnings:** `cargo build` and `cargo clippy` clean. No `#[allow]` to silence a real problem.
- **File size caps** (`validate.sh`): non-test `.rs` 600 LOC, `.slint` 500 LOC, `lib.rs`/`mod.rs` re-exports only and under 100 LOC.
- **TDD red-first is mandatory.** Never write production code without a test that failed first. Watch the assertion fail — a compile error is not a RED (#743). Paste the FAILED line in the commit or issue comment.
- **Ported code carries attribution.** Every file ported from Ruxguitar starts with a header naming the origin file and the Apache-2.0 licence. `LICENSES/Apache-2.0-ruxguitar.txt` ships at the repo root.
- **No new dependency without checking `Cargo.toml` first.** New deps go in `[workspace.dependencies]` and are referenced with `.workspace = true`.
- **Never `git add -A`.** Stage explicit paths.
- **Branch:** `feat/issue-864`. Push after every commit, then `gh issue comment 864` with hash, files and test result.
- **Spec:** `docs/superpowers/specs/2026-08-02-issue-864-score-reader-design.md`.

---

## File Structure

**New crate `crates/score`** — domain model and file readers.

| File | Responsibility |
|---|---|
| `src/lib.rs` | re-exports only |
| `src/model/song.rs` | `Song`, `SongInfo`, `MeasureHeader`, `Tempo`, `TimeSignature`, `KeySignature` |
| `src/model/track.rs` | `Track`, `Measure`, `Voice`, `Beat`, `Note`, `Duration` |
| `src/model/effects.rs` | `NoteEffect`, `BendEffect`, `BendPoint`, `HarmonicEffect`, `SlideType`, `GraceEffect`, `TrillEffect`, `TremoloPickingEffect`, `BeatEffects`, `BeatStroke` |
| `src/read/mod.rs` | `read(path) -> Result<Song, ScoreError>`, version sniffing |
| `src/read/gp345/header.rs` | song info, measure headers, track headers for GP3/4/5 |
| `src/read/gp345/music.rs` | measures, beats, notes, effects for GP3/4/5 |
| `src/read/gp345/primitives.rs` | byte/int/string readers, `encoding_rs` decoding |
| `src/read/gp67/archive.rs` | `.gpx` BCFS container and `.gp` zip container |
| `src/read/gp67/builder.rs` | GPIF XML → `Song` |
| `src/error.rs` | `ScoreError` |
| `tests/fixtures/` | `.gp3`–`.gp8` sample files (ported from Ruxguitar `test-files/`) |

**New crate `crates/score-layout`** — engraving.

| File | Responsibility |
|---|---|
| `src/lib.rs` | re-exports only |
| `src/primitive.rs` | `Primitive`, `PathSeg`, `TextStyle`, `ElementRef` |
| `src/page.rs` | `Page`, `System`, `StaffBlock`, `LayoutOptions`, `lay_out()` |
| `src/metrics.rs` | Bravura metrics loaded via `smufl`; staff space unit conversions |
| `src/tab/measure.rs` | tablature: strings, fret numbers, beat spacing, bar lines, repeats |
| `src/tab/effects.rs` | tablature: bends, slides, harmonics, tapping, strokes, annotations |
| `src/staff/notes.rs` | notation: note heads, stems, ledger lines, accidentals, rests, dots |
| `src/staff/beams.rs` | notation: beam grouping and slopes, ties, slurs |
| `src/spacing.rs` | shared horizontal spacing: one beat column serves both layers |
| `src/system.rs` | system and page breaking |
| `tests/golden/` | serialized expected primitives per fixture |

**New crate `crates/score-play`** — not phase 1. Created in phase 2.

**Modified: `crates/application`**

| File | Change |
|---|---|
| `src/command/score.rs` | new — `ScoreCommand` |
| `src/command.rs` | add `Score(ScoreCommand)` variant and the `pub mod`/`pub use` |
| `src/score_state.rs` | new — loaded song, selected track, library path |
| `src/local_dispatcher_score.rs` | new — handlers |
| `src/local_dispatcher_score_tests.rs` | new |

**Modified: `crates/adapter-gui`**

| File | Change |
|---|---|
| `ui/pages/score_window.slint` | new — `ScoreWindow` + `ScorePanel` |
| `ui/components/score_globals.slint` | new — `ScoreBridge` global, `ScorePrimitive` struct |
| `ui/components/score_canvas.slint` | new — the repeater that draws primitives |
| `ui/fonts/Bravura.otf` | new asset |
| `src/score_wiring.rs` | new — window open/close, callbacks → dispatcher |
| `src/score_view.rs` | new — `Page` → `ScorePrimitive` model rows |
| `src/score_view_tests.rs` | new |
| `src/desktop_app.rs` | instantiate `ScoreWindow`, wire the top-bar icon |

**Modified: docs** — `docs/screens.md`, `docs/architecture.md`, `docs/testing.md`, `docs/config-taxonomy.md`.

---

### Task 1: The `score` crate and its domain model

The model comes first because every later task types against it. It is ported from Ruxguitar's `src/parser/model.rs`, keeping field names so the parser port stays a mechanical translation.

**Files:**
- Create: `crates/score/Cargo.toml`, `crates/score/src/lib.rs`, `crates/score/src/model/song.rs`, `crates/score/src/model/track.rs`, `crates/score/src/model/effects.rs`, `crates/score/src/model/mod.rs`, `crates/score/src/error.rs`
- Create: `crates/score/tests/model_tests.rs`
- Modify: `Cargo.toml` (workspace members + `nom`, `encoding_rs`, `zip`, `roxmltree` in `[workspace.dependencies]`)
- Create: `LICENSES/Apache-2.0-ruxguitar.txt`

**Interfaces:**
- Consumes: nothing.
- Produces:
```rust
pub struct Song {
    pub info: SongInfo,
    pub tracks: Vec<Track>,
    pub measure_headers: Vec<MeasureHeader>,
    pub tempo: i32,
}
pub struct Track {
    pub number: i32,
    pub name: String,
    pub strings: Vec<(i8, i8)>,   // (string number, tuning as MIDI note)
    pub measures: Vec<Measure>,
    pub offset: i32,
    pub channel_index: usize,
}
pub struct Measure { pub header_index: usize, pub voices: Vec<Voice> }
pub struct Voice { pub beats: Vec<Beat>, pub directions: Option<()> }
pub struct Beat {
    pub start: i64,
    pub duration: Duration,
    pub notes: Vec<Note>,
    pub effect: BeatEffects,
    pub status: BeatStatus,
}
pub struct Note {
    pub value: i16,           // fret
    pub velocity: i16,
    pub string: i8,
    pub effect: NoteEffect,
    pub kind: NoteType,
}
pub struct Duration { pub value: u8, pub dotted: bool, pub tuplet: Tuplet }
impl Track { pub fn string_count(&self) -> usize }
impl Duration { pub fn ticks(&self) -> i64 }
```

- [ ] **Step 1: Write the failing test**

`crates/score/tests/model_tests.rs`:

```rust
use score::model::{Duration, Tuplet, Track};

#[test]
fn quarter_note_is_960_ticks() {
    let quarter = Duration { value: 4, dotted: false, tuplet: Tuplet::none() };
    assert_eq!(quarter.ticks(), 960);
}

#[test]
fn dotted_quarter_is_one_and_a_half_quarters() {
    let dotted = Duration { value: 4, dotted: true, tuplet: Tuplet::none() };
    assert_eq!(dotted.ticks(), 1440);
}

#[test]
fn eighth_note_triplet_is_two_thirds_of_an_eighth() {
    let triplet = Duration { value: 8, dotted: false, tuplet: Tuplet { enters: 3, times: 2 } };
    assert_eq!(triplet.ticks(), 320);
}

#[test]
fn string_count_follows_the_tuning() {
    let seven_string = Track {
        strings: vec![(1, 64), (2, 59), (3, 55), (4, 50), (5, 45), (6, 40), (7, 35)],
        ..Track::empty()
    };
    assert_eq!(seven_string.string_count(), 7);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p score --test model_tests`
Expected: FAIL — the crate does not exist yet (`error: package ID specification 'score' did not match any packages`). Once the crate skeleton exists, the failure must become an assertion failure or a missing-item error on `ticks()`.

- [ ] **Step 3: Create the crate and port the model**

`crates/score/Cargo.toml`:

```toml
[package]
name = "score"
version.workspace = true
edition.workspace = true

[dependencies]
thiserror.workspace = true
serde = { workspace = true, optional = true }

[features]
default = []
serde = ["dep:serde"]
```

Add `"crates/score"` to the workspace `members` list in the root `Cargo.toml`.

`crates/score/src/lib.rs` (re-exports only, under 100 LOC):

```rust
//! Score domain model and file readers (#864).
//!
//! Format-independent: a `.gp5` and a MusicXML file describing the same music
//! produce the same [`Song`].

pub mod error;
pub mod model;

pub use error::ScoreError;
pub use model::{Beat, Duration, Measure, Note, Song, Track, Voice};
```

Port the type definitions from Ruxguitar `src/parser/model.rs` into the three
`model/` files, split by the table in *File Structure*. Keep field names. Each
file gets the attribution header:

```rust
//! Ported from Ruxguitar `src/parser/model.rs` (https://github.com/agourlay/ruxguitar),
//! Apache-2.0 — see LICENSES/Apache-2.0-ruxguitar.txt.
```

`ticks()` uses the Guitar Pro tick base of 960 per quarter note:

```rust
pub const QUARTER_TICKS: i64 = 960;

impl Duration {
    pub fn ticks(&self) -> i64 {
        let mut ticks = (QUARTER_TICKS * 4) / i64::from(self.value);
        if self.dotted {
            ticks = ticks * 3 / 2;
        }
        if self.tuplet.enters > 1 {
            ticks = ticks * i64::from(self.tuplet.times) / i64::from(self.tuplet.enters);
        }
        ticks
    }
}
```

Add `Track::empty()` behind `#[cfg(test)]`-free code (test helpers used by the
integration test must be public), or provide `Default` and use `..Default::default()`
in the test — pick one and use it consistently across all later tasks.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p score --test model_tests`
Expected: 4 passed.

- [ ] **Step 5: Fetch the Apache-2.0 notice**

Download the Apache-2.0 text to `LICENSES/Apache-2.0-ruxguitar.txt` and add a
line to the repo `README.md` third-party section naming Ruxguitar as the origin
of the parser, MIDI and tablature-layout code.

- [ ] **Step 6: Commit**

```bash
git add crates/score Cargo.toml LICENSES/Apache-2.0-ruxguitar.txt README.md
git commit -m "feat(#864): score domain model, ported from Ruxguitar"
git push
```

---

### Task 2: Read GP5 files

GP5 first: it is the most common format and its reader covers the GP3/GP4 code
path with version branches, so Task 3 becomes small.

**Files:**
- Create: `crates/score/src/read/mod.rs`, `crates/score/src/read/gp345/mod.rs`, `crates/score/src/read/gp345/primitives.rs`, `crates/score/src/read/gp345/header.rs`, `crates/score/src/read/gp345/music.rs`
- Create: `crates/score/tests/read_gp5_tests.rs`
- Create: `crates/score/tests/fixtures/` (copy from Ruxguitar `test-files/`)
- Modify: `crates/score/Cargo.toml` (add `nom`, `encoding_rs`), `crates/score/src/lib.rs`

**Interfaces:**
- Consumes: `Song`, `Track`, `Measure`, `Beat`, `Note` from Task 1.
- Produces:
```rust
pub fn read(path: &std::path::Path) -> Result<Song, ScoreError>;
pub fn read_bytes(bytes: &[u8]) -> Result<Song, ScoreError>;
pub enum ScoreError {
    Io(std::io::Error),
    UnsupportedFormat { detected: String },
    Malformed { at: usize, reason: String },
}
```

- [ ] **Step 1: Write the failing test**

`crates/score/tests/read_gp5_tests.rs`:

```rust
use std::path::Path;

#[test]
fn reads_a_gp5_song_header() {
    let song = score::read(Path::new("tests/fixtures/demo.gp5")).expect("gp5 must parse");
    assert_eq!(song.info.name, "Demo Song");
    assert!(!song.tracks.is_empty(), "a song has at least one track");
}

#[test]
fn reads_measures_and_notes() {
    let song = score::read(Path::new("tests/fixtures/demo.gp5")).unwrap();
    let track = &song.tracks[0];
    assert_eq!(track.string_count(), 6, "standard guitar track");
    let first_measure = &track.measures[0];
    let first_beat = &first_measure.voices[0].beats[0];
    assert!(!first_beat.notes.is_empty(), "the first beat sounds a note");
    let note = &first_beat.notes[0];
    assert!((0..=24).contains(&note.value), "fret in range, got {}", note.value);
    assert!((1..=6).contains(&note.string), "string in range, got {}", note.string);
}

#[test]
fn rejects_a_file_that_is_not_guitar_pro() {
    let err = score::read_bytes(b"this is not a guitar pro file").unwrap_err();
    assert!(
        matches!(err, score::ScoreError::UnsupportedFormat { .. }),
        "expected UnsupportedFormat, got {err:?}"
    );
}
```

Copy fixtures from the Ruxguitar checkout (`test-files/`) into
`crates/score/tests/fixtures/`, and adjust the asserted song name to the fixture
you copy — read it once with a hex dump or with Ruxguitar itself and hard-code
the real value. Do not assert on a name you did not verify.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p score --test read_gp5_tests`
Expected: FAIL — `score::read` does not exist.

- [ ] **Step 3: Port the GP3/4/5 reader**

Port Ruxguitar `src/parser/gp345/primitive_parser.rs` → `read/gp345/primitives.rs`,
`src/parser/gp345/song_parser.rs` → `read/gp345/header.rs`, and
`src/parser/gp345/music_parser.rs` → `read/gp345/music.rs`.

`song_parser.rs` is 1114 LOC and `music_parser.rs` is 583 LOC; the 600-LOC cap
means `header.rs` must be split further — put song info, lyrics and page setup in
`header.rs` and the measure-header/track-header readers in a sibling
`read/gp345/tracks.rs`. Do not weaken the cap.

`read/mod.rs` sniffs the version from the leading version string and dispatches:

```rust
pub fn read_bytes(bytes: &[u8]) -> Result<Song, ScoreError> {
    let version = sniff_version(bytes)?;
    match version {
        GpVersion::V3 | GpVersion::V4 | GpVersion::V5 => gp345::read(bytes, version),
        other => Err(ScoreError::UnsupportedFormat { detected: format!("{other:?}") }),
    }
}
```

Port Ruxguitar's own parser tests (`src/parser/song_parser_tests.rs`, 795 LOC)
into `crates/score/tests/read_gp5_tests.rs` and sibling files as you go — they
are the regression suite for the formats and must not be dropped.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p score`
Expected: all pass, including the ported Ruxguitar cases.

- [ ] **Step 5: Commit**

```bash
git add crates/score
git commit -m "feat(#864): read GP5 files into the score model"
git push
```

---

### Task 3: Read GP3 and GP4

**Files:**
- Modify: `crates/score/src/read/mod.rs`, `crates/score/src/read/gp345/*`
- Create: `crates/score/tests/read_gp34_tests.rs`
- Create: fixtures `demo.gp3`, `demo.gp4`

**Interfaces:**
- Consumes: `read_bytes` from Task 2.
- Produces: nothing new — same `read` entry point, more accepted inputs.

- [ ] **Step 1: Write the failing test**

```rust
use std::path::Path;

#[test]
fn reads_gp4() {
    let song = score::read(Path::new("tests/fixtures/demo.gp4")).expect("gp4 must parse");
    assert!(!song.tracks.is_empty());
    assert!(!song.tracks[0].measures.is_empty());
}

#[test]
fn reads_gp3() {
    let song = score::read(Path::new("tests/fixtures/demo.gp3")).expect("gp3 must parse");
    assert!(!song.tracks.is_empty());
    assert!(!song.tracks[0].measures.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p score --test read_gp34_tests`
Expected: FAIL — `UnsupportedFormat` for both, because `read_bytes` only accepts V5 so far.

- [ ] **Step 3: Add the version branches**

Extend the `match` in `read_bytes` to accept `V3` and `V4`, and port the
version-conditional branches from Ruxguitar's parser (they are already written
as `if version >= GpVersion::V4 { … }` style checks — carry them across as-is).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p score`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/score
git commit -m "feat(#864): read GP3 and GP4 files"
git push
```

---

### Task 4: Read GP6 (`.gpx`) and GP7/GP8 (`.gp`)

Different container entirely: `.gpx` is the BCFS virtual filesystem, `.gp` is a
zip holding `Content/score.gpif` XML.

**Files:**
- Create: `crates/score/src/read/gp67/mod.rs`, `archive.rs`, `document.rs`, `builder.rs`, `bit_reader.rs`
- Create: `crates/score/tests/read_gp678_tests.rs`
- Create: fixtures `demo.gpx`, `demo.gp`
- Modify: `crates/score/Cargo.toml` (add `zip`, `roxmltree`), `crates/score/src/read/mod.rs`

**Interfaces:**
- Consumes: `Song` model, `ScoreError`.
- Produces: nothing new at the API surface.

- [ ] **Step 1: Write the failing test**

```rust
use std::path::Path;

#[test]
fn reads_gpx_gp6() {
    let song = score::read(Path::new("tests/fixtures/demo.gpx")).expect("gpx must parse");
    assert!(!song.tracks.is_empty());
    assert!(!song.tracks[0].measures.is_empty());
}

#[test]
fn reads_gp7_or_gp8() {
    let song = score::read(Path::new("tests/fixtures/demo.gp")).expect("gp must parse");
    assert!(!song.tracks.is_empty());
    assert!(!song.tracks[0].measures.is_empty());
}

#[test]
fn a_zip_without_a_gpif_is_rejected() {
    // A valid zip that is not a Guitar Pro archive must be a clean error,
    // never a panic.
    let bytes = std::fs::read("tests/fixtures/not-a-score.zip").unwrap();
    assert!(score::read_bytes(&bytes).is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p score --test read_gp678_tests`
Expected: FAIL — `UnsupportedFormat`.

- [ ] **Step 3: Port the GP6/7/8 reader**

Port Ruxguitar `src/parser/gp67/{archive,bit_reader,document,document_reader,file_system,song_builder}.rs`.
`song_builder.rs` is 849 LOC — split it: XML element readers in `document.rs`,
model construction in `builder.rs`, each under 600 LOC.

Extend the dispatch in `read_bytes`: a leading `PK\x03\x04` means a zip
(GP7/GP8), a leading `BCFS`/`BCFZ` means GP6, otherwise fall through to the
version-string sniff of Task 2.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p score`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/score
git commit -m "feat(#864): read GP6, GP7 and GP8 files"
git push
```

---

### Task 5: `score-layout` crate — primitives and the tablature staff

This is the first task of the engraving engine. It produces the empty tab staff:
six horizontal lines, the tuning label, and bar lines. Notes come in Task 6.

**Files:**
- Create: `crates/score-layout/Cargo.toml`, `src/lib.rs`, `src/primitive.rs`, `src/page.rs`, `src/tab/mod.rs`, `src/tab/measure.rs`
- Create: `crates/score-layout/tests/tab_staff_tests.rs`
- Modify: root `Cargo.toml` (members)

**Interfaces:**
- Consumes: `score::Song`, `score::Track`.
- Produces:
```rust
pub struct LayoutOptions {
    pub page_width: f32,        // logical px
    pub staff_space: f32,       // one staff space in px; the unit everything scales from
    pub show_tab: bool,
    pub show_notation: bool,
}
impl Default for LayoutOptions { /* page_width 1000.0, staff_space 8.0, both true */ }

pub struct Page { pub systems: Vec<System>, pub height: f32 }
pub struct System { pub y: f32, pub height: f32, pub primitives: Vec<Primitive> }

pub enum Primitive {
    Glyph { code: char, x: f32, y: f32, size: f32, element: ElementRef },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, width: f32, element: ElementRef },
    Curve { segments: Vec<PathSeg>, width: f32, element: ElementRef },
    Text { content: String, x: f32, y: f32, size: f32, bold: bool, element: ElementRef },
    Rect { x: f32, y: f32, width: f32, height: f32, filled: bool, element: ElementRef },
}

pub enum PathSeg { MoveTo { x: f32, y: f32 }, LineTo { x: f32, y: f32 },
                   CubicTo { x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32 } }

/// Which model element a primitive came from — the whole hit-testing story.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ElementRef {
    pub measure: u32,
    pub voice: u8,
    pub beat: u32,
    pub note: u8,      // u8::MAX when the primitive is not a note
}

pub fn lay_out(song: &score::Song, track_index: usize, opts: &LayoutOptions) -> Vec<Page>;
```

- [ ] **Step 1: Write the failing test**

`crates/score-layout/tests/tab_staff_tests.rs`:

```rust
use score_layout::{lay_out, LayoutOptions, Primitive};

fn demo_song() -> score::Song {
    score::read(std::path::Path::new("../score/tests/fixtures/demo.gp5")).unwrap()
}

#[test]
fn a_six_string_track_draws_six_tab_lines_per_system() {
    let song = demo_song();
    let opts = LayoutOptions { show_notation: false, ..Default::default() };
    let pages = lay_out(&song, 0, &opts);
    let first = &pages[0].systems[0];

    let horizontals = first.primitives.iter().filter(|p| matches!(
        p, Primitive::Line { y1, y2, .. } if (y1 - y2).abs() < f32::EPSILON
    )).count();

    assert_eq!(horizontals, 6, "one line per string");
}

#[test]
fn tab_lines_are_evenly_spaced() {
    let song = demo_song();
    let opts = LayoutOptions { show_notation: false, staff_space: 8.0, ..Default::default() };
    let pages = lay_out(&song, 0, &opts);

    let mut ys: Vec<f32> = pages[0].systems[0].primitives.iter().filter_map(|p| match p {
        Primitive::Line { y1, y2, .. } if (y1 - y2).abs() < f32::EPSILON => Some(*y1),
        _ => None,
    }).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let gaps: Vec<f32> = ys.windows(2).map(|w| w[1] - w[0]).collect();
    for gap in &gaps {
        assert!((gap - gaps[0]).abs() < 0.01, "uneven tab line spacing: {gaps:?}");
    }
}

#[test]
fn every_measure_ends_with_a_bar_line() {
    let song = demo_song();
    let opts = LayoutOptions { show_notation: false, ..Default::default() };
    let pages = lay_out(&song, 0, &opts);

    let measure_count = song.tracks[0].measures.len();
    let bar_lines: usize = pages.iter().flat_map(|p| &p.systems).flat_map(|s| &s.primitives)
        .filter(|p| matches!(p, Primitive::Line { x1, x2, .. } if (x1 - x2).abs() < f32::EPSILON))
        .count();

    assert!(bar_lines >= measure_count, "expected at least {measure_count} bar lines, got {bar_lines}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p score-layout --test tab_staff_tests`
Expected: FAIL — crate does not exist, then assertion failures once the skeleton
returns empty pages.

- [ ] **Step 3: Implement the tab staff**

`crates/score-layout/Cargo.toml` depends on `score = { path = "../score" }` and
nothing UI-related. Add `"crates/score-layout"` to workspace members.

`src/page.rs` holds `lay_out`, which walks measures, asks `tab::measure` for each
measure's width and primitives, and breaks systems when the accumulated width
exceeds `opts.page_width`. Port the geometry decisions from Ruxguitar
`src/ui/canvas_measure.rs` — `draw_measure_vertical_line`, `beat_natural_width`,
`overhead_width` — replacing `frame.stroke(...)` calls with pushes onto a
`Vec<Primitive>`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p score-layout`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/score-layout Cargo.toml
git commit -m "feat(#864): score-layout crate and the tablature staff"
git push
```

---

### Task 6: Tablature notes and beat spacing

**Files:**
- Modify: `crates/score-layout/src/tab/measure.rs`
- Create: `crates/score-layout/src/spacing.rs`
- Create: `crates/score-layout/tests/tab_notes_tests.rs`

**Interfaces:**
- Consumes: `Primitive`, `ElementRef`, `LayoutOptions` from Task 5.
- Produces:
```rust
/// Horizontal position of every beat in a measure, shared by both staves so
/// the tab and the notation stay column-aligned.
pub struct BeatColumns { pub xs: Vec<f32>, pub width: f32 }
pub fn columns_for(measure: &score::Measure, opts: &LayoutOptions) -> BeatColumns;
```

- [ ] **Step 1: Write the failing test**

```rust
use score_layout::{lay_out, LayoutOptions, Primitive};

#[test]
fn a_fret_number_is_drawn_for_every_note() {
    let song = score::read(std::path::Path::new("../score/tests/fixtures/demo.gp5")).unwrap();
    let track = &song.tracks[0];
    let note_count: usize = track.measures.iter()
        .flat_map(|m| &m.voices)
        .flat_map(|v| &v.beats)
        .map(|b| b.notes.len())
        .sum();

    let opts = LayoutOptions { show_notation: false, ..Default::default() };
    let pages = lay_out(&song, 0, &opts);

    let fret_texts = pages.iter().flat_map(|p| &p.systems).flat_map(|s| &s.primitives)
        .filter(|p| matches!(p, Primitive::Text { element, .. } if element.note != u8::MAX))
        .count();

    assert_eq!(fret_texts, note_count, "one fret number per note");
}

#[test]
fn a_fret_number_sits_on_its_string_line() {
    let song = score::read(std::path::Path::new("../score/tests/fixtures/demo.gp5")).unwrap();
    let opts = LayoutOptions { show_notation: false, staff_space: 8.0, ..Default::default() };
    let pages = lay_out(&song, 0, &opts);
    let system = &pages[0].systems[0];

    let mut line_ys: Vec<f32> = system.primitives.iter().filter_map(|p| match p {
        Primitive::Line { y1, y2, .. } if (y1 - y2).abs() < f32::EPSILON => Some(*y1),
        _ => None,
    }).collect();
    line_ys.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let first_fret = system.primitives.iter().find_map(|p| match p {
        Primitive::Text { y, element, .. } if element.note != u8::MAX => Some(*y),
        _ => None,
    }).expect("at least one fret number");

    assert!(
        line_ys.iter().any(|line_y| (line_y - first_fret).abs() < 0.6),
        "fret at y={first_fret} is not on any string line {line_ys:?}"
    );
}

#[test]
fn longer_notes_get_more_horizontal_room() {
    use score_layout::columns_for;
    let opts = LayoutOptions::default();
    let whole = measure_of_one(1);      // helper: a measure with a single whole note
    let sixteenth = measure_of_one(16);
    assert!(
        columns_for(&whole, &opts).width > columns_for(&sixteenth, &opts).width,
        "a whole note must take more space than a sixteenth"
    );
}
```

Write `measure_of_one(duration_value: u8) -> score::Measure` as a test helper in
the same file: one voice, one beat, one note on string 1 fret 0, with the given
`Duration.value`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p score-layout --test tab_notes_tests`
Expected: FAIL — `fret_texts` is 0, and `columns_for` does not exist.

- [ ] **Step 3: Implement notes and spacing**

Port `draw_beat`, `draw_note` and `beat_natural_width` from Ruxguitar
`src/ui/canvas_measure.rs`. Spacing is proportional to duration, as there: the
natural width grows with the note value, and `columns_for` returns the running x
of each beat plus the total.

Every emitted primitive carries the `ElementRef` of the beat/note it came from —
this is what Task 11's hit-testing and phase 3's editor read.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p score-layout`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/score-layout
git commit -m "feat(#864): tablature notes and duration-proportional spacing"
git push
```

---

### Task 7: Tablature effects

Bends, slides, hammer-ons, harmonics, tapping, palm mutes, strokes, repeats and
alternate endings — the guitar vocabulary. This is the largest single port.

**Files:**
- Create: `crates/score-layout/src/tab/effects.rs`
- Modify: `crates/score-layout/src/tab/measure.rs`
- Create: `crates/score-layout/tests/tab_effects_tests.rs`
- Create: fixture `crates/score/tests/fixtures/effects.gp5` (a file exercising bend, slide, harmonic, palm mute, repeat)

**Interfaces:**
- Consumes: `Primitive`, `ElementRef`, `BeatColumns`.
- Produces: no new public API — richer primitive output.

- [ ] **Step 1: Write the failing test**

```rust
use score_layout::{lay_out, LayoutOptions, Primitive};

fn effects_song() -> score::Song {
    score::read(std::path::Path::new("../score/tests/fixtures/effects.gp5")).unwrap()
}

#[test]
fn a_bend_draws_a_curve() {
    let opts = LayoutOptions { show_notation: false, ..Default::default() };
    let pages = lay_out(&effects_song(), 0, &opts);
    let curves = pages.iter().flat_map(|p| &p.systems).flat_map(|s| &s.primitives)
        .filter(|p| matches!(p, Primitive::Curve { .. }))
        .count();
    assert!(curves > 0, "a bend must produce at least one curve primitive");
}

#[test]
fn a_palm_muted_beat_is_annotated() {
    let opts = LayoutOptions { show_notation: false, ..Default::default() };
    let pages = lay_out(&effects_song(), 0, &opts);
    let has_pm = pages.iter().flat_map(|p| &p.systems).flat_map(|s| &s.primitives)
        .any(|p| matches!(p, Primitive::Text { content, .. } if content == "P.M."));
    assert!(has_pm, "palm mute must be annotated above the staff");
}

#[test]
fn two_bends_on_the_same_beat_do_not_overlap() {
    // Ruxguitar's multiple_bend_conflicts logic: stacked bends are offset so
    // their curves stay readable.
    let opts = LayoutOptions { show_notation: false, ..Default::default() };
    let pages = lay_out(&effects_song(), 0, &opts);

    let mut bend_starts: Vec<(u32, u32, f32)> = pages.iter().flat_map(|p| &p.systems)
        .flat_map(|s| &s.primitives)
        .filter_map(|p| match p {
            Primitive::Curve { segments, element, .. } => match segments.first() {
                Some(score_layout::PathSeg::MoveTo { y, .. }) => Some((element.measure, element.beat, *y)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    bend_starts.sort_by(|a, b| a.partial_cmp(b).unwrap());

    for pair in bend_starts.windows(2) {
        if pair[0].0 == pair[1].0 && pair[0].1 == pair[1].1 {
            assert!((pair[0].2 - pair[1].2).abs() > 0.5,
                "two bends on the same beat start at the same y — they will overlap");
        }
    }
}
```

Author `effects.gp5` in Guitar Pro or TuxGuitar and verify by reading it with
the Task 2 reader that it really contains a bend with multiple points, a palm
mute and a repeat before relying on it.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p score-layout --test tab_effects_tests`
Expected: FAIL — no curves and no "P.M." text.

- [ ] **Step 3: Port the effects drawing**

Port from Ruxguitar `src/ui/canvas_measure.rs`: `draw_bend`,
`multiple_bend_conflicts`, `above_note_effect_annotation`,
`inlined_note_effect_annotation`, `draw_stroke_arrow`, `draw_open_section`,
`draw_open_repeat`, `draw_close_repeat`, `draw_repeat_dots`,
`draw_alternative_ending`, `draw_end_section`.

`effects.rs` must stay under 600 LOC; if the port exceeds it, split repeats and
section markers into `tab/sections.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p score-layout`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/score-layout crates/score/tests/fixtures/effects.gp5
git commit -m "feat(#864): tablature effects — bends, annotations, repeats"
git push
```

---

### Task 8: Bravura metrics and the notation staff

The first task of the new work: standard notation. Start with the empty staff
and the clef, so the font pipeline is proven before note layout depends on it.

**Files:**
- Create: `crates/score-layout/src/metrics.rs`, `src/staff/mod.rs`, `src/staff/notes.rs`
- Create: `crates/score-layout/assets/bravura_metadata.json` (from the Bravura release)
- Create: `crates/score-layout/tests/staff_tests.rs`
- Modify: `crates/score-layout/Cargo.toml` (add `smufl`)

**Interfaces:**
- Consumes: `Primitive`, `LayoutOptions`.
- Produces:
```rust
/// Bravura's engraving defaults and glyph anchors, in staff spaces.
pub struct Metrics { /* … */ }
impl Metrics {
    pub fn bravura() -> &'static Metrics;              // parsed once, cached
    pub fn staff_line_thickness(&self) -> f32;         // in staff spaces
    pub fn stem_thickness(&self) -> f32;
    pub fn beam_thickness(&self) -> f32;
    pub fn stem_up_se(&self, glyph: char) -> (f32, f32);  // stem attachment
}

/// SMuFL codepoints used by this crate.
pub mod glyph {
    pub const G_CLEF: char = '\u{E050}';
    pub const G_CLEF_8VB: char = '\u{E052}';
    pub const NOTEHEAD_BLACK: char = '\u{E0A4}';
    pub const NOTEHEAD_HALF: char = '\u{E0A3}';
    pub const NOTEHEAD_WHOLE: char = '\u{E0A2}';
    pub const FLAG_8TH_UP: char = '\u{E240}';
    pub const REST_QUARTER: char = '\u{E4E5}';
    pub const ACCIDENTAL_SHARP: char = '\u{E262}';
    pub const ACCIDENTAL_FLAT: char = '\u{E260}';
    pub const ACCIDENTAL_NATURAL: char = '\u{E261}';
    pub const AUGMENTATION_DOT: char = '\u{E1E7}';
}
```

- [ ] **Step 1: Write the failing test**

```rust
use score_layout::{glyph, lay_out, LayoutOptions, Metrics, Primitive};

#[test]
fn bravura_metrics_load() {
    let m = Metrics::bravura();
    // Bravura's engraving defaults: staff lines are thin, stems thinner still.
    assert!(m.staff_line_thickness() > 0.0 && m.staff_line_thickness() < 0.2,
        "staff line thickness out of range: {}", m.staff_line_thickness());
    assert!(m.stem_thickness() > 0.0 && m.stem_thickness() < 0.2);
    assert!(m.beam_thickness() > m.stem_thickness(), "beams are thicker than stems");
}

#[test]
fn a_notation_system_draws_five_staff_lines() {
    let song = score::read(std::path::Path::new("../score/tests/fixtures/demo.gp5")).unwrap();
    let opts = LayoutOptions { show_tab: false, show_notation: true, ..Default::default() };
    let pages = lay_out(&song, 0, &opts);

    let horizontals = pages[0].systems[0].primitives.iter().filter(|p| matches!(
        p, Primitive::Line { y1, y2, .. } if (y1 - y2).abs() < f32::EPSILON
    )).count();

    assert_eq!(horizontals, 5, "a standard staff has five lines");
}

#[test]
fn a_guitar_staff_opens_with_a_treble_clef_octave_down() {
    let song = score::read(std::path::Path::new("../score/tests/fixtures/demo.gp5")).unwrap();
    let opts = LayoutOptions { show_tab: false, show_notation: true, ..Default::default() };
    let pages = lay_out(&song, 0, &opts);

    let first_glyph = pages[0].systems[0].primitives.iter().find_map(|p| match p {
        Primitive::Glyph { code, .. } => Some(*code),
        _ => None,
    }).expect("the system starts with a clef");

    assert_eq!(first_glyph, glyph::G_CLEF_8VB, "guitar notation reads an octave down");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p score-layout --test staff_tests`
Expected: FAIL — `Metrics` does not exist.

- [ ] **Step 3: Implement metrics and the staff**

Download `bravura_metadata.json` from the Bravura release into
`crates/score-layout/assets/` and `include_str!` it. Parse with the `smufl`
crate, exposing only the values this crate uses through `Metrics` — do not leak
the `smufl` types into the public API, so the metadata source stays swappable.

`staff/notes.rs` draws the five lines, the clef glyph, and the bar lines,
reusing the same `BeatColumns` from Task 6 so both staves align.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p score-layout`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/score-layout
git commit -m "feat(#864): Bravura metrics and the notation staff"
git push
```

---

### Task 9: Notation note heads, stems, accidentals and rests

**Files:**
- Modify: `crates/score-layout/src/staff/notes.rs`
- Create: `crates/score-layout/src/staff/pitch.rs` (MIDI note → staff position + accidental)
- Create: `crates/score-layout/tests/staff_notes_tests.rs`

**Interfaces:**
- Consumes: `Metrics`, `glyph`, `BeatColumns`.
- Produces:
```rust
/// Vertical position on the staff, in half staff spaces from the top line,
/// plus the accidental the note needs in the current key.
pub struct StaffPos { pub steps: i32, pub accidental: Option<char> }
pub fn staff_pos(midi_note: i16, key: score::KeySignature) -> StaffPos;
```

- [ ] **Step 1: Write the failing test**

```rust
use score_layout::{glyph, staff_pos, Primitive};

// Guitar notation is written an octave up from sounding pitch, so the open
// high E (MIDI 64) sits in the top space of the treble staff.
#[test]
fn open_high_e_sits_in_the_top_space() {
    let pos = staff_pos(64, score::KeySignature::c_major());
    assert_eq!(pos.steps, 1, "E5 written position");
    assert_eq!(pos.accidental, None, "E is diatonic in C major");
}

#[test]
fn f_sharp_gets_a_sharp_in_c_major() {
    let pos = staff_pos(66, score::KeySignature::c_major());
    assert_eq!(pos.accidental, Some(glyph::ACCIDENTAL_SHARP));
}

#[test]
fn f_sharp_needs_no_accidental_in_g_major() {
    let pos = staff_pos(66, score::KeySignature::g_major());
    assert_eq!(pos.accidental, None, "F# is in the key signature of G major");
}

#[test]
fn a_quarter_note_draws_a_black_head_and_a_stem() {
    let song = score::read(std::path::Path::new("../score/tests/fixtures/demo.gp5")).unwrap();
    let opts = score_layout::LayoutOptions { show_tab: false, ..Default::default() };
    let pages = score_layout::lay_out(&song, 0, &opts);

    let heads = pages.iter().flat_map(|p| &p.systems).flat_map(|s| &s.primitives)
        .filter(|p| matches!(p, Primitive::Glyph { code, .. } if *code == glyph::NOTEHEAD_BLACK))
        .count();
    assert!(heads > 0, "quarter notes draw black note heads");

    let stems = pages.iter().flat_map(|p| &p.systems).flat_map(|s| &s.primitives)
        .filter(|p| matches!(p, Primitive::Line { x1, x2, element, .. }
            if (x1 - x2).abs() < f32::EPSILON && element.note != u8::MAX))
        .count();
    assert!(stems > 0, "stems are vertical lines attached to notes");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p score-layout --test staff_notes_tests`
Expected: FAIL — `staff_pos` does not exist.

- [ ] **Step 3: Implement pitch mapping and note drawing**

`staff_pos` converts a MIDI note to a diatonic staff step (C major reference)
and decides the accidental against the key signature. The sounding pitch of a
tab note is `track.strings[string - 1].1 + fret`; written pitch for guitar is
that plus 12.

Note heads use `NOTEHEAD_WHOLE`/`NOTEHEAD_HALF`/`NOTEHEAD_BLACK` by duration.
Stem direction: down for notes above the middle line, up otherwise. Stem length
is 3.5 staff spaces from the head, extended to reach the middle line when the
note is far outside the staff. Attachment x comes from `Metrics::stem_up_se`.
Ledger lines every other step beyond the staff. Augmentation dots for dotted
durations. Rests for `BeatStatus::Rest`.

Add `KeySignature::c_major()` and `KeySignature::g_major()` constructors to the
`score` crate if they do not exist — a test-only helper is not acceptable here,
these are real domain constructors.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p score-layout && cargo test -p score`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/score-layout crates/score
git commit -m "feat(#864): note heads, stems, accidentals and rests"
git push
```

---

### Task 10: Beams, ties and system breaking

**Files:**
- Create: `crates/score-layout/src/staff/beams.rs`, `crates/score-layout/src/system.rs`
- Modify: `crates/score-layout/src/page.rs`
- Create: `crates/score-layout/tests/beams_tests.rs`, `crates/score-layout/tests/system_tests.rs`

**Interfaces:**
- Consumes: everything above.
- Produces:
```rust
/// A run of consecutive beamed beats within one measure.
pub struct BeamGroup { pub beats: Vec<u32>, pub level: u8 }
pub fn beam_groups(measure: &score::Measure, signature: score::TimeSignature) -> Vec<BeamGroup>;
```

- [ ] **Step 1: Write the failing test**

`beams_tests.rs`:

```rust
use score_layout::beam_groups;

#[test]
fn four_eighths_in_four_four_beam_in_two_pairs() {
    let measure = measure_of_eighths(4);   // helper: four eighth notes
    let groups = beam_groups(&measure, score::TimeSignature::four_four());
    assert_eq!(groups.len(), 2, "eighths beam per beat in 4/4, got {groups:?}");
    assert_eq!(groups[0].beats, vec![0, 1]);
    assert_eq!(groups[1].beats, vec![2, 3]);
}

#[test]
fn a_quarter_note_is_never_beamed() {
    let measure = measure_of_quarters(4);
    assert!(beam_groups(&measure, score::TimeSignature::four_four()).is_empty());
}

#[test]
fn a_beam_is_drawn_as_a_thick_line_between_stems() {
    let song = score::read(std::path::Path::new("../score/tests/fixtures/demo.gp5")).unwrap();
    let opts = score_layout::LayoutOptions { show_tab: false, ..Default::default() };
    let pages = score_layout::lay_out(&song, 0, &opts);
    let m = score_layout::Metrics::bravura();

    let beams = pages.iter().flat_map(|p| &p.systems).flat_map(|s| &s.primitives)
        .filter(|p| matches!(p, score_layout::Primitive::Line { width, .. }
            if *width >= m.beam_thickness() * 8.0 * 0.9))
        .count();
    assert!(beams > 0, "beamed eighths must draw beam lines");
}
```

`system_tests.rs`:

```rust
use score_layout::{lay_out, LayoutOptions};

#[test]
fn measures_wrap_into_systems_at_the_page_width() {
    let song = score::read(std::path::Path::new("../score/tests/fixtures/demo.gp5")).unwrap();
    let narrow = LayoutOptions { page_width: 400.0, ..Default::default() };
    let wide = LayoutOptions { page_width: 2000.0, ..Default::default() };

    let narrow_systems: usize = lay_out(&song, 0, &narrow).iter().map(|p| p.systems.len()).sum();
    let wide_systems: usize = lay_out(&song, 0, &wide).iter().map(|p| p.systems.len()).sum();

    assert!(narrow_systems > wide_systems,
        "a narrower page needs more systems: {narrow_systems} vs {wide_systems}");
}

#[test]
fn no_primitive_spills_past_the_page_width() {
    let song = score::read(std::path::Path::new("../score/tests/fixtures/demo.gp5")).unwrap();
    let opts = LayoutOptions { page_width: 800.0, ..Default::default() };
    for page in lay_out(&song, 0, &opts) {
        for system in &page.systems {
            for p in &system.primitives {
                assert!(max_x(p) <= 800.0 + 0.5, "primitive overflows the page: {p:?}");
            }
        }
    }
}

#[test]
fn tab_and_notation_share_the_same_beat_columns() {
    let song = score::read(std::path::Path::new("../score/tests/fixtures/demo.gp5")).unwrap();
    let opts = LayoutOptions { show_tab: true, show_notation: true, ..Default::default() };
    let pages = lay_out(&song, 0, &opts);
    let system = &pages[0].systems[0];

    // Group the x of every note primitive by (measure, beat); a beat must have
    // one x, whichever staff drew it.
    let mut by_beat: std::collections::HashMap<(u32, u32), Vec<f32>> = Default::default();
    for p in &system.primitives {
        if let Some((element, x)) = note_anchor(p) {
            by_beat.entry((element.measure, element.beat)).or_default().push(x);
        }
    }
    for ((measure, beat), xs) in by_beat {
        let first = xs[0];
        for x in &xs {
            assert!((x - first).abs() < 2.0,
                "measure {measure} beat {beat} is not column-aligned: {xs:?}");
        }
    }
}
```

Write `max_x` and `note_anchor` as helpers in the test file.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p score-layout --test beams_tests --test system_tests`
Expected: FAIL — `beam_groups` missing; alignment assertion fails.

- [ ] **Step 3: Implement beaming and system breaking**

`beam_groups` groups consecutive beats shorter than a quarter within one beat of
the time signature. Beam slope follows the first and last stem tips, clamped to
one staff space of rise so it never looks like a ramp. Ties connect notes with
`NoteType::Tie` to the previous note of the same string/pitch as a `Curve`.

`system.rs` accumulates measure widths and starts a new system when the next
measure would exceed `page_width`, then stretches the measures of the closed
system to fill the line. Both staves of a system come from the same
`BeatColumns`, which is what makes the alignment test pass.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p score-layout`
Expected: all pass.

- [ ] **Step 5: Add golden tests**

Create `crates/score-layout/tests/golden_tests.rs` that lays out each fixture
and compares a stable serialization of the primitives (rounded to 2 decimals)
against a checked-in `tests/golden/<fixture>.txt`. Generate the files once,
inspect them by eye, and commit them. A layout change then shows as a diff.

- [ ] **Step 6: Commit**

```bash
git add crates/score-layout
git commit -m "feat(#864): beams, ties, system breaking and layout golden tests"
git push
```

---

### Task 11: `ScoreCommand` and the dispatcher

**Files:**
- Create: `crates/application/src/command/score.rs`, `crates/application/src/score_state.rs`, `crates/application/src/local_dispatcher_score.rs`, `crates/application/src/local_dispatcher_score_tests.rs`
- Modify: `crates/application/src/command.rs`, `crates/application/src/lib.rs`, `crates/application/Cargo.toml` (add `score`)

**Interfaces:**
- Consumes: `score::Song`, `score::read`.
- Produces:
```rust
pub enum ScoreCommand {
    OpenScore { path: String },
    CloseScore,
    SelectScoreTrack { index: usize },
}

pub struct ScoreState {
    pub song: Option<std::sync::Arc<score::Song>>,
    pub path: Option<String>,
    pub track_index: usize,
}
impl ScoreState {
    pub fn track_names(&self) -> Vec<String>;
}
```
Plus `Command::Score(ScoreCommand)`.

- [ ] **Step 1: Write the failing test**

`crates/application/src/local_dispatcher_score_tests.rs`:

```rust
use crate::command::{Command, ScoreCommand};

#[test]
fn opening_a_score_loads_the_song_and_selects_the_first_track() {
    let dispatcher = test_dispatcher();
    let fixture = fixture_path("demo.gp5");

    dispatcher.dispatch(Command::Score(ScoreCommand::OpenScore { path: fixture.clone() })).unwrap();

    let state = dispatcher.score_state();
    assert!(state.song.is_some(), "the song must be loaded");
    assert_eq!(state.path.as_deref(), Some(fixture.as_str()));
    assert_eq!(state.track_index, 0, "the first track is selected on open");
}

#[test]
fn selecting_a_track_out_of_range_is_rejected_and_leaves_the_selection_alone() {
    let dispatcher = test_dispatcher();
    dispatcher.dispatch(Command::Score(ScoreCommand::OpenScore {
        path: fixture_path("demo.gp5"),
    })).unwrap();

    let result = dispatcher.dispatch(Command::Score(ScoreCommand::SelectScoreTrack { index: 99 }));

    assert!(result.is_err(), "an out-of-range track must be rejected");
    assert_eq!(dispatcher.score_state().track_index, 0);
}

#[test]
fn opening_a_file_that_is_not_a_score_reports_an_error_and_keeps_the_previous_song() {
    let dispatcher = test_dispatcher();
    dispatcher.dispatch(Command::Score(ScoreCommand::OpenScore {
        path: fixture_path("demo.gp5"),
    })).unwrap();

    let result = dispatcher.dispatch(Command::Score(ScoreCommand::OpenScore {
        path: fixture_path("not-a-score.zip"),
    }));

    assert!(result.is_err());
    assert!(dispatcher.score_state().song.is_some(), "the loaded song survives a failed open");
}

#[test]
fn closing_clears_the_state() {
    let dispatcher = test_dispatcher();
    dispatcher.dispatch(Command::Score(ScoreCommand::OpenScore {
        path: fixture_path("demo.gp5"),
    })).unwrap();

    dispatcher.dispatch(Command::Score(ScoreCommand::CloseScore)).unwrap();

    let state = dispatcher.score_state();
    assert!(state.song.is_none());
    assert!(state.path.is_none());
}
```

Follow the existing `local_dispatcher_metronome_tests.rs` for how
`test_dispatcher()` and fixture paths are built in this crate — reuse its
helpers rather than inventing new ones. Fixtures resolve through
`CARGO_MANIFEST_DIR`, never an absolute machine path.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p application score`
Expected: FAIL — `ScoreCommand` does not exist.

- [ ] **Step 3: Implement the command, state and handlers**

`command/score.rs` mirrors the shape and doc-comment style of
`command/metronome.rs`. Register the module and the `pub use` in `command.rs`,
and add `Score(ScoreCommand)` to the `Command` enum.

`local_dispatcher_score.rs` implements the handlers: `OpenScore` calls
`score::read`, replaces the state on success and returns the error untouched on
failure; `SelectScoreTrack` validates against `song.tracks.len()`; `CloseScore`
resets.

Reading a file is I/O and runs on the dispatcher thread, never on an audio
thread — there is no audio in phase 1 at all.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p application`
Expected: all pass, including the existing suite.

- [ ] **Step 5: Verify the command schema still generates**

Run: `cargo test -p application command_schema`
Expected: PASS. `ScoreCommand` variants appear in the generated schema, which is
what gives MCP and gRPC the same surface.

- [ ] **Step 6: Commit**

```bash
git add crates/application
git commit -m "feat(#864): ScoreCommand, score state and dispatcher handlers"
git push
```

---

### Task 12: Draw the score in Slint

**Files:**
- Create: `crates/adapter-gui/ui/components/score_globals.slint`, `ui/components/score_canvas.slint`, `ui/pages/score_window.slint`, `ui/fonts/Bravura.otf`
- Create: `crates/adapter-gui/src/score_view.rs`, `src/score_view_tests.rs`, `src/score_wiring.rs`
- Modify: `crates/adapter-gui/src/desktop_app.rs`, `crates/adapter-gui/Cargo.toml` (add `score`, `score-layout`), `crates/adapter-gui/src/lib.rs`

**Interfaces:**
- Consumes: `score_layout::{Page, Primitive, PathSeg}`, `application::ScoreState`.
- Produces:
```rust
/// One Slint model row. Kind: 0 glyph, 1 line, 2 curve, 3 text, 4 rect.
pub struct ScoreRow {
    pub kind: i32,
    pub x: f32, pub y: f32, pub width: f32, pub height: f32,
    pub text: String,        // glyph char or text content
    pub size: f32,
    pub commands: String,    // SVG path data for kind 2
    pub stroke: f32,
}
pub fn rows_for(page: &score_layout::Page) -> Vec<ScoreRow>;
pub fn svg_commands(segments: &[score_layout::PathSeg]) -> String;
```

- [ ] **Step 1: Write the failing test**

`crates/adapter-gui/src/score_view_tests.rs`:

```rust
use crate::score_view::{rows_for, svg_commands, ScoreRow};
use score_layout::PathSeg;

#[test]
fn a_curve_becomes_an_svg_path_string() {
    let segments = vec![
        PathSeg::MoveTo { x: 10.0, y: 20.0 },
        PathSeg::CubicTo { x1: 15.0, y1: 10.0, x2: 25.0, y2: 10.0, x: 30.0, y: 20.0 },
    ];
    assert_eq!(svg_commands(&segments), "M 10 20 C 15 10 25 10 30 20");
}

#[test]
fn every_primitive_becomes_exactly_one_row() {
    let song = score::read(&fixture("demo.gp5")).unwrap();
    let pages = score_layout::lay_out(&song, 0, &Default::default());
    let primitive_count: usize = pages[0].systems.iter().map(|s| s.primitives.len()).sum();

    assert_eq!(rows_for(&pages[0]).len(), primitive_count);
}

#[test]
fn a_glyph_row_carries_the_codepoint_as_text() {
    let song = score::read(&fixture("demo.gp5")).unwrap();
    let pages = score_layout::lay_out(&song, 0, &Default::default());
    let rows = rows_for(&pages[0]);

    let glyph_row = rows.iter().find(|r| r.kind == 0).expect("a clef glyph exists");
    assert_eq!(glyph_row.text.chars().count(), 1, "a glyph row holds one character");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p adapter-gui score_view`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Implement the view mapping and the Slint canvas**

`score_view.rs` converts a `Page` into rows. `svg_commands` formats with the
shortest representation that round-trips (`{:.0}` when the value is integral,
otherwise `{:.2}`) — match the test's expected string exactly.

`ui/components/score_globals.slint`:

```slint
// #864 — the score panel reads its state from this global rather than from
// bound properties, so the same panel serves the standalone window and the
// inline (fullscreen/touch) placement. Same reasoning as MetronomeBridge (#14)
// and DiPanel (#749).
export struct ScorePrimitive {
    kind: int,          // 0 glyph, 1 line, 2 curve, 3 text, 4 rect
    x: length, y: length, width: length, height: length,
    text: string,
    size: length,
    commands: string,
    stroke: length,
}

export global ScoreBridge {
    in-out property <[ScorePrimitive]> primitives;
    in-out property <[string]> track-names;
    in-out property <int> track-index;
    in-out property <string> title;
    in-out property <length> content-height;
    in-out property <bool> loaded;

    callback open-score();
    callback close-score();
    callback select-track(int);
}
```

`ui/components/score_canvas.slint` draws the rows. Bravura is imported the way
the other fonts are (`import "../fonts/Bravura.otf";` — see
`ui/app-window.slint` for the existing `CooperHewitt-Semibold.ttf` import):

```slint
import "../fonts/Bravura.otf";
import { ScoreBridge, ScorePrimitive } from "score_globals.slint";

export component ScoreCanvas inherits Rectangle {
    background: #fbfaf7;

    for p[i] in ScoreBridge.primitives: Rectangle {
        x: p.x; y: p.y;

        if p.kind == 0: Text {
            text: p.text;
            font-family: "Bravura";
            font-size: p.size;
            color: #14171c;
        }
        if p.kind == 1: Rectangle {
            width: p.width; height: p.height;
            background: #14171c;
        }
        if p.kind == 2: Path {
            commands: p.commands;
            stroke: #14171c;
            stroke-width: p.stroke;
        }
        if p.kind == 3: Text {
            text: p.text;
            font-size: p.size;
            color: #14171c;
        }
        if p.kind == 4: Rectangle {
            width: p.width; height: p.height;
            background: #14171c;
        }
    }
}
```

A horizontal line is emitted as `kind: 1` with its height set to the stroke
width — a `Rectangle` beats a `Path` for the hundreds of straight lines in a
page.

`ui/pages/score_window.slint` wraps `ScoreCanvas` in a `ScrollView`, adds the
header (title, track select, open/close buttons) and stays under 500 LOC. The
track picker is a root-level inline panel with a click-outside backdrop —
**never `PopupWindow`** (#749/#761).

`score_wiring.rs` follows `metronome_wiring.rs`: one `wire()` entry point per
`AppWindow + ScoreWindow` pair, callbacks dispatching `ScoreCommand`, and a
refresh that recomputes `lay_out` and republishes the model whenever the state
changes or the window resizes.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p adapter-gui score_view`
Expected: all pass.

- [ ] **Step 5: Render it and look at it**

Run: `cargo run -p slint-render -- crates/adapter-gui/ui/pages/score_window.slint`
(see `.claude/skills/openrig-tooling/SKILL.md` for the exact invocation).
Open the PNG. The staff lines must be even, the clef must sit on the correct
line, fret numbers must be centred on their string, and nothing may overlap.
**Do not proceed while it looks wrong** — that is the whole point of phase 1.

- [ ] **Step 6: Commit**

```bash
git add crates/adapter-gui
git commit -m "feat(#864): draw tab and notation in Slint"
git push
```

---

### Task 13: Open a file, pick a track, and prove the clicks land

**Files:**
- Modify: `crates/adapter-gui/src/score_wiring.rs`, `ui/pages/score_window.slint`, `src/desktop_app.rs`
- Create: `crates/adapter-gui/src/score_wiring_tests.rs`
- Modify: `crates/application/src/app_config_persist.rs` (add `scores_path`), `crates/adapter-gui/ui/pages/settings/*` (library path field)
- Modify: `crates/adapter-gui/translations/*.po` via `scripts/extract-translations.sh`

**Interfaces:**
- Consumes: everything above.
- Produces: `scores_path` in `config.yaml`.

- [ ] **Step 1: Write the failing interaction test**

A headless render proves layout only; it does not prove a click lands (#749,
#761). Use `i-slint-backend-testing` the way the existing UI interaction tests
in this crate do.

```rust
#[test]
fn choosing_a_track_dispatches_select_score_track() {
    let harness = ScoreHarness::new_with_song("demo.gp5");

    harness.click("score-track-select");
    harness.click("score-track-row-1");

    assert_eq!(
        harness.dispatched(),
        vec![Command::Score(ScoreCommand::SelectScoreTrack { index: 1 })],
    );
}

#[test]
fn the_track_picker_rows_are_reachable() {
    // The #749/#761 lesson: a PopupWindow's rows never receive clicks. This
    // test fails if the picker is ever moved back into one.
    let harness = ScoreHarness::new_with_song("demo.gp5");
    harness.click("score-track-select");
    assert!(harness.element_is_clickable("score-track-row-1"));
}

#[test]
fn selecting_a_track_redraws_the_canvas() {
    let harness = ScoreHarness::new_with_song("demo.gp5");
    let before = harness.primitive_count();

    harness.select_track(1);

    assert_ne!(before, harness.primitive_count(),
        "a different track must produce a different drawing");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p adapter-gui score_wiring`
Expected: FAIL — the callbacks are not wired.

- [ ] **Step 3: Wire the open flow, the picker and the library path**

Open uses `rfd` for the file dialog as the DI loop's "Choose file…" does, opening
in `scores_path`. Add `scores_path` to the system config next to `plugins_path`
and `presets_path` — the library lives on the machine and does not travel with
the `.openrig` (ADR 0003). Add the field to the Settings screen's *System*
section.

Add the top-bar icon opening `ScoreWindow` in windowed mode and inline in
fullscreen/touch, following `desktop_app.rs` lines around the metronome window's
instantiation.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p adapter-gui`
Expected: all pass.

- [ ] **Step 5: Refresh translations**

Run: `scripts/extract-translations.sh`, then fill every new msgid in all nine
`.po` files. English is the reference. A missing translation renders the raw key
in the UI.

- [ ] **Step 6: Commit**

```bash
git add crates/adapter-gui crates/application
git commit -m "feat(#864): open a score, pick a track, score library path"
git push
```

---

### Task 14: Documentation

Documentation is part of the task, in the same branch — not a follow-up.

**Files:**
- Modify: `docs/screens.md`, `docs/architecture.md`, `docs/testing.md`, `docs/config-taxonomy.md`, `docs/development/file-organization.md`

- [ ] **Step 1: Write the docs**

`docs/screens.md` — a **Score** entry alongside Metronome and Tuner: what it
shows, where it opens (own window on desktop, inline in fullscreen/touch), that
it reads gp3–gp8, that the track picker is a root-level panel, and that phase 1
has no audio.

`docs/architecture.md` — the three crates, their boundaries, and the rule that
`score-layout` has no UI dependency.

`docs/config-taxonomy.md` — `scores_path` as a system setting, with the ADR 0003
reasoning.

`docs/testing.md` — the golden-primitive test convention and where the fixtures
live.

`docs/development/file-organization.md` — one row for the score crates.

- [ ] **Step 2: Verify no doc claims anything phase 1 does not do**

Read each new paragraph against the code. No mention of playback, transport,
editing or preset linking — those are phases 2 and 3.

- [ ] **Step 3: Commit**

```bash
git add docs
git commit -m "docs(#864): score reader phase 1"
git push
```

---

### Task 15: Close out phase 1

- [ ] **Step 1: Full build and test**

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```
Expected: builds clean, all tests pass, zero warnings.

- [ ] **Step 2: Render the final state**

Render `score_window.slint` again and attach the PNG to the issue comment.

- [ ] **Step 3: Comment on the issue**

`gh issue comment 864` with the commit range, the files, the test counts, and
the validation checklist for the owner — the `git fetch && git checkout
feat/issue-864 && git pull` block plus numbered checkboxes covering only what he
must judge with his eyes: does the notation look right, does the tab look right,
do they line up, is it readable at a glance from a metre away, does his own `.gp`
library open.

Do **not** put "read the spec" or "review the plan" in that checklist — those are
working artifacts, not deliverables he reviews.

---

## Self-Review

**Spec coverage.** Phase 1 of the spec asks for: import gp3–gp8 (Tasks 2–4),
tablature + standard notation rendering (Tasks 5–10), scroll and system layout
(Tasks 10, 12), track selection (Tasks 11, 13), no audio (nothing in this plan
opens a stream). The spec's `score-play` crate and the `LinkPreset` command are
phase 2 and correctly absent. `scores_path` persistence (Task 13) and the
testing strategy (golden tests Task 10, interaction tests Task 13, headless
render Task 12) are covered. The spec's MusicXML/MIDI import is *not* in phase 1
— it is listed under the `score` crate's eventual responsibility and belongs
with the export work in phase 3; no task claims it.

**Placeholder scan.** No TBD/TODO. Every code step carries real code. Two places
name a judgement rather than a value on purpose: the asserted song title in Task
2 (must be read from the fixture, and the step says so explicitly rather than
inventing a name) and the fixture authoring in Task 7 (must be verified with the
Task 2 reader before use).

**Type consistency.** `Primitive`, `ElementRef`, `PathSeg`, `LayoutOptions`,
`Page`, `System` keep the same names and fields from Task 5 through Task 12.
`BeatColumns`/`columns_for` introduced in Task 6 are consumed in Tasks 8 and 10.
`Metrics::bravura()` from Task 8 is used in Task 10's beam test. `ScoreCommand`
variants named in Task 11 are the ones dispatched in Task 13. `ScoreRow.kind`
values (0–4) match the `p.kind` branches in the Slint canvas.
