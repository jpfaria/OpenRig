# Course format and app client (#866, spec 2 of 5)

**Status:** closed — design and spec approved by the owner, 2026-08-02.

**Series:** #866 covers five specs. This is the second — the first is the
practice evaluator, and specs 3–5 (video delivery, Hub courses, teacher tool)
follow. Distribution rides on the OpenRig Hub of #309.

## Problem

A course today is a playlist plus a folder of downloads. The student watches a
video in one window, hunts for the tab in another, sets a metronome in a third,
and guesses when to move on. Every one of those tools already exists inside
OpenRig — metronome (#14), looper (#323), DI loop (#771), tuner, score reader
(#864) — and spec 1 adds the one thing no video platform has: something that
hears the take and scores it.

What is missing is the thread. A lesson should arrive knowing the tempo it wants,
the tone it wants, the passage it drills and the standard it expects, and it
should set all of that up when the student opens it.

## Goals

1. Define what a course and a lesson **are**, as data.
2. Play a lesson: video, material, exercise, and the rig it needs.
3. Track progress so the app can answer *am I ready for the next one?*
4. Keep the student's own work untouched.
5. Make the format identical whether the video is on YouTube or on our own
   infrastructure, so migrating hosting changes one field.

## Non-goals

- The video infrastructure itself (spec 3).
- The catalogue, accounts, purchase and publishing (spec 4).
- The teacher's authoring tool (spec 5).
- Judging the take — that is spec 1, consumed here as a library.

## The model

```
Course
 ├── metadata: id, title, author, version, language, level
 ├── rubric: the course-wide default (spec 1 `Rubric`)
 └── Module[]
      └── Lesson[]
           ├── video: VideoRef
           ├── text: markdown
           ├── material: score refs, backing tracks, images
           ├── rig: RigSetup
           ├── exercises: Exercise[]  (spec 1)
           ├── rubric: RubricPatch?   (overrides the course default)
           └── gate: Gate
```

### `VideoRef` — one field decides the player

```rust
pub enum VideoRef {
    /// Plays through the YouTube IFrame Player API in its own window.
    /// Extracting the stream would violate YouTube's terms; the IFrame API in
    /// a WebView is the sanctioned path for a native app.
    YouTube { id: String, start_s: Option<u32> },
    /// Plays natively inside the Slint window via GStreamer (spec 3).
    Hls { url: String, poster: Option<String> },
    /// A file inside the course package — used by the teacher tool before
    /// publishing, and by offline course exports.
    LocalFile { path: String },
}
```

The rest of the format does not know which one it got. A course that starts on
YouTube and later moves to our own hosting changes `VideoRef` and nothing else.

### `RigSetup` — how a lesson arms the rig

```rust
pub struct RigSetup {
    pub tempo_bpm: Option<u32>,
    pub time_signature: Option<(u32, u32)>,
    pub subdivision: Option<String>,
    pub count_in: Option<bool>,
    pub chain_preset: Option<PresetRef>,   // a preset shipped with the course
    pub tuning: Option<Vec<i8>>,
    pub score: Option<ScoreRef>,           // file + measure range (#864)
    pub loop_region: Option<(i64, i64)>,   // ticks, for the looper/score loop
    pub di_loop: Option<String>,           // a DI file shipped with the course
}
```

Every field is optional: a lesson that only wants a tempo sets a tempo.

### `Gate` — progression

```rust
pub struct Gate {
    pub blocking: bool,          // the lesson refuses to hand over the next one
    pub required_score: f32,     // resolved against the rubric's pass mark
}

pub enum LessonStatus { NotAttempted, InTraining, NeedsWork, Mastered, Skipped }
```

Default is non-blocking: the student moves on carrying `NeedsWork`, and the app
knows where to send them back. A lesson may declare itself blocking (spec 1).

## The lesson's own project

**The student's open project is never modified.** Opening a lesson opens the
lesson's own project — its preset, tempo, loop and score — and **inherits the
student's I/O bindings**, because the audio interface belongs to the machine and
not to the course (ADR 0003). Closing the lesson returns to whatever the student
had open, exactly as they left it.

Inside the lesson the student may change anything: swap the amp, drop the tempo,
turn off a block. Those changes persist per-lesson so the next session resumes
where they were, and a **revert action restores the lesson's original rig** in
one step. The teacher's setup is therefore a starting point, not a cage.

Implementation note: this is a project like any other. The lesson project lives
under the course's local cache, not in the student's projects folder, so it
never appears in the launcher's recent list as if it were their own work.

## Course package

Follows the `.orplugin` shape of #309 — a directory with a manifest, which the
Hub can serve as an archive:

```
guitar-foundations/
├── manifest.yaml          # id, title, author, version, license, rubric
├── modules/
│   └── 01-first-chords/
│       ├── lesson.yaml    # one file per lesson: video, text, rig, gate
│       ├── exercises/
│       │   └── am-changes.yaml
│       └── material/
│           ├── am-changes.gp5
│           └── backing.wav
├── presets/
│   └── clean-practice.yaml
└── LICENSE
```

Data only — no executable code, the same rule #309 sets for plugins, and the
same reason: a course cannot be a supply-chain vector.

`lesson.yaml` is authored by the teacher tool (spec 5) but is plain YAML a human
can write and diff, which is what makes the format reviewable.

## App client

### Screens

**Courses** — the library: installed courses, progress per course, and the entry
point to the Hub catalogue (spec 4). Reached from the top bar.

**Course** — the path: modules and lessons with their status
(`NotAttempted`/`InTraining`/`NeedsWork`/`Mastered`/`Skipped`), the next lesson
highlighted, and lessons the student should revisit called out — this is the
screen that answers *am I ready?*

**Lesson** — the working surface: video (in-window for HLS, in its own window
for YouTube), the teacher's text, the score with the drilled passage, the
exercise transport, and the live verdicts from spec 1. The rig controls the lesson
armed are the app's own — the metronome is the metronome, not a copy of it.

Placement and rules follow the existing surfaces: state on a bridge global the
panels read directly, pickers as root-level panels and **never `PopupWindow`**
(#749/#761), and every control dispatching a `Command`.

### Video playback

Two implementations behind one interface:

```rust
pub trait LessonVideo {
    fn open(&mut self, video: &VideoRef) -> Result<()>;
    fn play(&mut self);
    fn pause(&mut self);
    fn seek(&mut self, seconds: f64);
    fn position(&self) -> f64;
    fn close(&mut self);
}
```

- **HLS** — GStreamer into a Slint `Image`, following Slint's official
  `gstreamer-player` example. Lives in the lesson window, next to the tab.
- **YouTube** — a `wry` WebView window running the IFrame Player API. Slint does
  not embed a WebView in the same window, so this is a separate window the app
  owns, positions and closes with the lesson. The IFrame API also gives the
  transport (`play`, `pause`, `seek`, `position`), so the same trait is
  satisfiable.

**Neither touches the audio path.** Video audio plays through the system's own
output, exactly as a browser would — it is not routed into a chain, does not
open an OpenRig stream, and cannot affect latency or xruns (invariants #4, #8).
A lesson that wants audio *inside* the rig ships a backing track and uses the
existing player, not the video.

### Progress and persistence (ADR 0003)

- **System** (`config.yaml`): `courses_path` (where installed courses live) and
  the student's rubric overrides.
- **Course cache**: the installed package plus each lesson's project.
- **Progress**: per lesson — status, best score, attempt count, last attempt —
  in system scope beside the overrides. Grades belong to the person and the
  machine; a `.openrig` sent to someone else does not carry them.

Nothing about a course is written into the student's `.openrig`.

### Commands

```
CourseCommand::InstallCourse { path or hub id }
            ::RemoveCourse { course }
            ::OpenCourse { course }
            ::OpenLesson { course, lesson }
            ::CloseLesson
            ::RevertLessonRig
            ::MarkLessonSkipped { lesson }
VideoCommand::PlayVideo | PauseVideo | SeekVideo { seconds }
```

Same law as everything else: MCP and a MIDI footswitch reach them like the GUI
does (#127). A footswitch that replays the exercise without putting the guitar
down is the point.

## Phases

1. **Local course, no video** — install from a folder, the path screen, lesson
   projects, rig arming, exercises wired to spec 1, progress. A course works
   end-to-end with the video field empty.
2. **YouTube video** — the `wry` window and the IFrame transport.
3. **HLS video** — the in-window GStreamer player (needs spec 3).
4. **Hub install** — replace the folder install with a Hub download (needs spec 4).

## Testing

- Course/lesson parsing: a valid package, a package missing a required field, a
  package referencing a file that is not there, an unknown `VideoRef` variant.
- `RigSetup` application: each field arms the right subsystem; an absent field
  changes nothing; revert restores the lesson's original.
- **The student's project is untouched:** open a project, open a lesson, change
  the lesson's rig, close the lesson, assert the original project is
  byte-identical.
- Progress transitions: every `LessonStatus` path including a blocking gate.
- Video: the trait's contract against a fake implementation; the GStreamer and
  WebView backends behind `OPENRIG_HW_TESTS=1`, since both need a real display.
- Interaction tests with `i-slint-backend-testing` for every control before any
  UI push (#749, #761).

## Risks

| Risk | Mitigation |
|---|---|
| A separate YouTube window is a poor experience | It is explicitly the cheap phase; the format makes the HLS migration a one-field change |
| GStreamer pulls a C toolchain into a pure-Rust build across macOS, Windows and Orange Pi ARM | Phase 3, behind a feature flag; a course with no HLS lesson does not need it built |
| `wry` and Slint fighting over the event loop | Prototype phase 2 against the real Slint event loop before committing to the window design |
| Course packages growing large with material | Video is remote by design; material is scores and backing tracks, which are small |
| Lesson projects piling up on disk | They live in the course cache and are removed with the course |
