# Score reader — Guitar Pro tab + standard notation (#864)

**Status:** closed — design and spec approved by the owner, 2026-08-02.

## Problem

OpenRig is a practice rig without anything to practise *from*. The metronome
(#14), the looper (#323) and the DI loop give a player tempo, layers and a
backing riff, but the notation still lives in another application — on the
standalone Orange Pi rig, on a machine that has no second screen and no DAW,
that means it lives nowhere.

The player already owns the material: Guitar Pro files. What is missing is a
surface inside OpenRig that reads them, shows them the way Guitar Pro does
(tablature **and** standard notation), plays the other instruments while the
guitar part stays silent for the player to fill in, and eventually lets them
write their own.

## Goals

1. Open any Guitar Pro file (`.gp3`, `.gp4`, `.gp5`, `.gpx`, `.gp`) and render
   it legibly. **Visual quality is a requirement, not a nice-to-have** — an ugly
   score is a failed feature.
2. Play the score through an isolated audio stream while the player's guitar
   runs through the rig untouched.
3. Edit and save scores, exporting back to formats other tools read.
4. Stay **pure Rust**. The build pipeline is Rust-only across macOS universal,
   Windows and Orange Pi ARM, and it stays that way.

## Non-goals

- Orchestral engraving. Guitar notation is a restricted case: one treble clef
  8vb (plus bass and drums for other tracks), no cross-staff beaming, no ossia,
  no figured bass, no historical notation.
- Score-following (listening to the player's audio and tracking position in the
  score). Possible later; not in this design.
- Automatic transcription of played audio into notation.
- Importing every notation format in existence. Guitar Pro is the input that
  matters; MusicXML and MIDI ride along because they are cheap once the domain
  model exists.

## Architecture

Three new crates, none of which know Slint exists, plus one GUI surface.

```
        .gp3/.gp4/.gp5/.gpx/.gp        MusicXML / MIDI
                    │                        │
                    └────────┬───────────────┘
                             ▼
                     ┌───────────────┐
                     │     score     │  domain model + import/export
                     └───────┬───────┘
                             │  Song
                 ┌───────────┴────────────┐
                 ▼                        ▼
        ┌────────────────┐       ┌────────────────┐
        │  score-layout  │       │   score-play   │
        │   engraving    │       │  MIDI + synth  │
        └───────┬────────┘       └───────┬────────┘
                │ Page{primitives}       │ isolated stream
                ▼                        ▼
        ┌──────────────────────────────────────────┐
        │  adapter-gui: score_window.slint         │
        │  draws primitives, computes no layout    │
        └──────────────────────────────────────────┘
```

### `score` — domain model and formats

Owns `Song`, `Track`, `Measure`, `Beat`, `Note`, tunings, and the note-effect
vocabulary (bend with its movement points, slide, hammer-on/pull-off, harmonic,
palm mute, vibrato, tremolo, grace note, tapping, stroke direction). Format
readers are modules behind one `read(path) -> Result<Song>` door; the model is
format-independent, so a `.gp5` and a MusicXML file that describe the same music
produce the same `Song`.

Exporters mirror it: the native OpenRig score format, `.gp7`/`.gp8` (XML inside
a zip — the tractable one), MusicXML, and MIDI.

### `score-layout` — the engraving engine

The heart of the feature and the part that decides whether it looks good.

```rust
pub fn lay_out(song: &Song, opts: &LayoutOptions) -> Vec<Page>;

pub struct Page { pub systems: Vec<System> }
pub struct System { pub staves: Vec<Staff>, pub primitives: Vec<Primitive>, … }

pub enum Primitive {
    Glyph { code: SmuflCodepoint, x: f32, y: f32, size: f32 },
    Line  { x1: f32, y1: f32, x2: f32, y2: f32, width: f32 },
    Curve { path: Vec<PathSeg>, width: f32 },
    Text  { content: String, x: f32, y: f32, style: TextStyle },
    Rect  { … },
}
```

Every primitive carries the id of the model element it came from, so hit-testing
is a lookup over the same output the renderer draws — the editor (phase 3) needs
no second data structure and no separate coordinate system.

Two layers:

- **Tablature.** Ported from Ruxguitar's `canvas_measure.rs` (see *Reuse*). It
  already resolves the hard cases: bend curves including multiple-bend collision
  handling, above-note vs inline effect annotations, repeats, alternate endings,
  time signatures, beat width from note duration.
- **Standard notation.** New. Bravura (SIL OFL, the SMuFL reference font) for
  glyphs, the [`smufl`](https://docs.rs/smufl) crate for the glyph metrics that
  make spacing correct rather than eyeballed (stem attachment points, glyph
  bounding boxes, engraving defaults such as staff line thickness and beam
  spacing). Layout responsibilities: note spacing within a measure, measure
  widths across a system, beaming and beam slopes, stem directions, accidentals,
  ledger lines, ties and slurs, rests, dots.

The two layers are laid out together and stay vertically aligned — the same beat
sits at the same x in the staff and in the tab, which is what makes the pair
readable.

**This crate has no UI dependency and is unit-tested in isolation.** That is
also the escape hatch: if standard-notation engraving ever needs to be swapped
for something else, it is swapped behind this API and nothing else moves.

### `score-play` — playback

`Song` → MIDI event list → `rustysynth` with an embedded soundfont (user-
replaceable). Ported from Ruxguitar's `midi_builder` / `midi_sequencer` /
`midi_player`, which already translate the guitar effect vocabulary into MIDI
(bends as pitch-wheel movements, palm mute as velocity/duration shaping, etc.).

Per-track mute/solo, tempo scaling, A/B loop region, and a position the UI reads
to draw the playback cursor. Speed change costs nothing here: the sequencer
advances its clock more slowly and the synth renders the same notes, so pitch is
untouched by construction — no time-stretching of audio is involved, unlike the
backing-track player of #324.

### `adapter-gui` — the screen

`score_window.slint` draws `score-layout`'s primitives through a repeater of
`Path`/`Text`/`Rectangle`. `score_wiring.rs` connects callbacks to the
dispatcher. **The screen computes no layout and holds no rules.**

Placement follows the metronome: a top-bar icon that opens its own window in
windowed desktop mode and renders inline over the chains page in
fullscreen/touch mode. Panel state lives on a `ScoreBridge` global the panel
reads directly, rather than being threaded through `AppWindow → DesktopMain/
TouchMain` — the same reasoning as `DiPanel` (#749) and `MetronomePanel` (#14).

Pickers (track list, output endpoint, soundfont) are root-level inline panels
with a click-outside backdrop. **Never `PopupWindow`** — its content does not
receive clicks in this app, confirmed twice (#749, #761).

## Reuse: Ruxguitar

[Ruxguitar](https://github.com/agourlay/ruxguitar) is a Guitar Pro tablature
player in Rust: 11.7k LOC, Apache-2.0 (one-way compatible with our GPL-3),
already built on `nom`, `rustysynth` and **`cpal`** — the same audio backend
OpenRig uses.

| Ruxguitar source | ~LOC | Ported into |
|---|---|---|
| `parser/gp345/*`, `parser/gp67/*`, `parser/model.rs` | ~4 000 | `score` |
| `audio/midi_builder/*`, `midi_sequencer.rs`, `midi_player.rs`, `playback_order.rs` | ~2 400 | `score-play` |
| `ui/canvas_measure.rs`, `ui/tablature.rs`, `ui/tuning.rs` | ~1 500 | `score-layout` |

Its drawing layer targets Iced's vector `Canvas` — strokes, fills and paths,
which map onto Slint `Path`/`Text` without a rewrite of the layout logic. The
port is a translation of the *drawing calls* into *emitted primitives*; the
geometry decisions come across intact.

Its own tests (`song_parser_tests.rs`, `midi_builder/tests.rs`, ~1 400 LOC) and
fixtures port with the code and become the regression suite.

The Apache-2.0 licence text and attribution ship with the ported code, and each
ported file carries a header naming its origin.

## Audio isolation

Score playback opens its **own stream** on the chosen output endpoint, exactly
as the metronome has since #14 and the DI since #808.

- No line is added to `process_output_f32`, `runtime_process_segment` or the
  output limiter.
- A chain rebuild, a live block edit or a chain failure cannot chop the
  playback; the playback cannot leak into a chain's buffers, its meters, or a
  recording taken off another output.
- Playback works with **no chain enabled** — the dispatcher's start door creates
  the runtime the way the DI's arm does (#808), so a freshly opened project
  still plays.
- The output is picked from the project's I/O binding output endpoints
  ("Binding · Endpoint"), not a raw device — same picker semantics as the
  metronome, so the score lands on the device *and channels* the project is
  configured with.

Invariant #4 applied literally: the score is one more independent stream.

## Commands

Every state change is a `Command` in `crates/application/src/command.rs`, so a
MIDI footswitch and an MCP client drive the score exactly as the on-screen
buttons do. This is the #127 lesson: state changes that live in a GUI event
handler are reachable from one callback and nowhere else.

```
ScoreCommand::Open(path)          SelectTrack(index)
             ::Close              ToggleTrackMute(index)
             ::Play               ToggleTrackSolo(index)
             ::Pause              SetOutput(endpoint)
             ::Stop               SetSpeed(factor)
             ::Seek(position)     SetCountIn(bars)
             ::SetLoopRegion(a, b)
             ::LinkPreset(chain, preset)
```

MIDI slots for `score_play_stop` and `score_seek_next/prev_section` follow the
looper's precedent in `docs/midi-profiles.md`.

Read side: `openrig://score` serves the same snapshot the window renders —
loaded song metadata, track list, transport state, position.

## Persistence (ADR 0003)

The rule is *"if I send this `.openrig` to another machine, must this value come
along?"*

- **System** (`config.yaml`): `scores_path` — where the score library lives,
  alongside `plugins_path` and `presets_path`. Also the soundfont path and the
  default output endpoint.
- **Project** (`.openrig`): the **link** only — which score the project opens
  with, and which chain preset is bound to it. Open the project for a song and
  the score comes up with the right tone already selected.

Transport state (playing, position, loop region) is runtime-only and never
written, like the metronome's `enabled`.

## Phases

Designed as one architecture, delivered in three.

### Phase 1 — read and draw

Import gp3–gp8, render tab + standard notation, scroll and system layout, track
selection. No audio. This is the phase that proves the visual bar; it is not
done until the rendered output stands next to Guitar Pro's without embarrassment.

### Phase 2 — play and study

`score-play` on an isolated stream, transport, A/B loop, speed, count-in,
playback cursor, per-track mute/solo, metronome and looper sync, preset link.

### Phase 3 — edit

Note entry and editing, undo/redo, native save, export to `.gp7`/`.gp8`,
MusicXML and MIDI.

Each phase is its own branch off this issue.

## Testing

- **`score-layout` golden tests.** Snapshot the emitted primitives for a fixture
  set of songs and assert on them. No UI, deterministic, and they catch a
  spacing regression the moment it happens.
- **Parser tests** against Guitar Pro fixtures across every supported version,
  ported from Ruxguitar.
- **`score-play` tests** on the generated MIDI event list — ported, plus new
  cases for mute/solo and loop regions.
- **Interaction tests** with `i-slint-backend-testing` for every control before
  any UI push. A headless render proves layout only; it does not prove a click
  lands (#749, #761).
- **Headless render** via `tools/slint-render` to inspect the drawing as a PNG
  before claiming it looks right.
- TDD red-first throughout, per `docs/testing.md`.

## Alternatives considered

### Verovio (rejected)

[Verovio](https://github.com/rism-digital/verovio) is a mature engraving library
with Rust FFI bindings ([verovioxide](https://github.com/music-comp/verovioxide))
and would supply professional-grade notation rendering for free. Rejected on
four counts:

1. **Its tablature is lute tablature.** Every tablature issue in its tracker
   concerns `tab.lute.german|french|italian` — the historical notation its
   musicological audience needs. A search of the tracker turns up nothing on
   guitar bends, palm mutes or slides, and rendering MusicXML with a TAB staff
   has an open bug ([#4414](https://github.com/rism-digital/verovio/issues/4414),
   July 2026). Guitar tablature is the part we need most.
2. **It does not read Guitar Pro.** Input is MEI/MusicXML/ABC/Humdrum, so a
   `.gp` → MusicXML/MEI converter would be ours regardless — and that converter
   is where the entire guitar vocabulary lives. The saving is smaller than it
   looks.
3. **It is C++.** Adopting it puts a statically linked C++ build into a pipeline
   that is pure Rust today, across macOS universal, Windows and Orange Pi ARM.
4. **It is read-only by nature.** Phase 3 through SVG re-render plus
   element-id hunting is indirect and slow.

The `score-layout` API keeps the door open: if standard-notation engraving turns
out to be more expensive than estimated, Verovio can be put behind that one
trait without touching the parser, the editor or the rest of the build.

### Rasterizing to an image (tiny-skia / vello) — rejected

Full typographic control, but re-render on every scroll, no GPU compositing, and
hit-testing built by hand. Worse than drawing natively in Slint on every axis
that matters here.

### Pure Slint layout (rejected)

Computing layout in `.slint` would put the engraving rules in the screen,
violating "the screen has no business logic", and make it untestable without an
`AppWindow`.

## Risks

| Risk | Mitigation |
|---|---|
| Standard-notation engraving is larger than estimated | Guitar is a restricted case; `smufl` supplies the metrics; the `score-layout` API allows a swap without touching anything else |
| Slint `Path` performance with thousands of primitives per page | Emit per-system, render only visible systems, cache system geometry; measure early in phase 1 |
| Ported Iced drawing does not map 1:1 onto Slint primitives | The port is of geometry decisions, not of draw calls; golden tests pin the output |
| Soundfont size on the Orange Pi image | Ship a small GM soundfont; the path is configurable for users who want a larger one |
| Guitar Pro format edge cases across versions | Ruxguitar's fixture suite ports with the parser |
