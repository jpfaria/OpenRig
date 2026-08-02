# Practice evaluator — listen to the player and score the take (#866, spec 1 of 5)

**Status:** closed — design and spec approved by the owner, 2026-08-02.

**Series:** spec 1 of #866 — the evaluator underneath the course platform. It stands alone: it works with a local exercise and no course at all.

## Problem

The owner's words: *"as aulas no Hotmart são soltas demais. tem que ficar
baixando arquivo. eu não sei como saber se estou pronto para a próxima fase."*

Two failures, and only the second is interesting.

The first is packaging: a lesson is a video plus a scatter of downloads — a
backing track, a tab, a PDF — none of it where the guitar is plugged in. That is
annoying, and it is solvable by any app with a folder.

The second is the real one: **no online course can tell you whether you have
actually learned the lesson.** It shows you the material and moves on. You decide
when you are ready, with no evidence, and you carry the weak fundamental forward
until something later collapses on top of it.

OpenRig can answer that question, and almost nothing else can, for one reason:
**it is already listening to the guitar.** The metronome (#14), the looper
(#323), the DI loop (#771), the tuner and the score reader (#864) are already the
practice bench. What is missing is a teacher — something that hears the take and
says *you have this* or *not yet, and here is what is wrong*.

## Goals

1. Score a take against a target on four axes: **pitch, timing, cleanliness,
   tempo**.
2. Make the standard **configurable data**, layered course → lesson → student.
3. Answer "am I ready for the next one?" with evidence instead of a guess.
4. Cost the audio path **nothing**.

## Non-goals

- Video playback, lesson packaging, the course path, its UI. A second spec
  covers the shell; this one builds the engine underneath it. The engine is
  identical whether the video is self-recorded, bought elsewhere, or absent.
- Blind polyphonic transcription. The evaluator always knows what was expected —
  see *Chord verification*.
- Recording the student's audio for later review.
- Judging technique that audio cannot see (posture, fingering choice, picking
  hand shape).

## Architecture

```
   guitar ─► chain input ─► InputTap (exists) ─► lock-free ring
                  │                                    │
                  ▼                                    ▼  worker thread
            the block graph                   ┌──────────────────┐
            (player's tone,                   │   feature-dsp    │
             untouched)                       │ onset · pitch ·  │
                                              │ chord · clean    │
                                              └────────┬─────────┘
                                                       │ PlayedEvent
                                       expected        ▼
   score::Song (#864) ──► Exercise ──────────► ┌──────────────────┐
   or hand-written                             │  practice-eval   │
                                               │ clock + elastic  │
                                               └────────┬─────────┘
                                                        │ Evaluation
                                                        ▼
                                                    UI · MCP
```

### `practice` — the domain

No DSP, no UI, no audio dependency.

```rust
pub struct Exercise {
    pub id: ExerciseId,
    pub title: String,
    pub expected: Vec<ExpectedEvent>,
    pub tuning: Vec<i8>,
    pub source: ExerciseSource,   // a score file + range, or hand-written
}

pub struct ExpectedEvent {
    pub at_tick: i64,             // position on the metronome grid
    pub duration_ticks: i64,
    pub target: Target,
    pub string: Option<i8>,       // when the exercise pins the fingering
}

pub enum Target {
    Note { midi: i16 },
    Chord { midi: Vec<i16> },
    Rest,
}
```

The **rubric** is the configurable standard:

```rust
pub struct Rubric {
    pub target_bpm: u32,
    pub pitch: Option<PitchRule>,        // None = this axis is not judged
    pub timing: Option<TimingRule>,
    pub cleanliness: Option<CleanRule>,
    pub tempo: Option<TempoRule>,
    pub pass_mark: f32,                  // 0.0..=1.0 weighted score to pass
    pub blocking: bool,                  // failing blocks the next lesson
    pub allow_student_override: bool,
}

pub struct TimingRule { pub tolerance_ms: f32, pub weight: f32 }
pub struct PitchRule { pub weight: f32, pub allow_octave_errors: bool }
pub struct CleanRule { pub weight: f32, pub min_sustain_ms: f32 }
pub struct TempoRule { pub weight: f32, pub min_fraction_of_target: f32 }
```

**Three layers, resolved in order:** the course supplies defaults, the lesson
overrides any field, and the student may loosen it *only if the lesson's
resolved rubric has `allow_student_override`*. The same precedence shape as
system vs project config (ADR 0003). Resolution is one pure function:

```rust
pub fn resolve(course: &Rubric, lesson: Option<&RubricPatch>, student: Option<&RubricPatch>)
    -> Rubric;
```

A student override never changes what counts as *passed* for a blocking lesson
unless the course allowed it — the flag is the whole permission model, so it is
checked in `resolve`, not at the call sites.

### `feature-dsp` — three new modules

Beside the existing `pitch_yin.rs` (YIN, monophonic) and `spectrum_fft.rs`
(63-band 1/6-octave).

**Onset detection** (`onset.rs`) — when a note was struck. Spectral-flux over
the same FFT frames the spectrum analyzer already computes, with a peak-picking
threshold. The onset instant is what timing is measured against; pitch alone
cannot say *when*.

**Chord verification** (`chord_match.rs`) — **not** blind transcription. The
evaluator always knows the target, so the question is narrow: *are the expected
partials present, and are the wrong ones absent?* For a target chord, sum the
energy in the bins around each expected fundamental and its first partials, and
compare against the energy in the bins of the plausible wrong notes (the same
shape a semitone off, the same chord with the wrong third). This is a template
match against a known target and is tractable exactly where open polyphonic
transcription is not.

**Cleanliness** (`note_quality.rs`) — did the note actually sound? Three
symptoms with different signatures: a **muted** note has an onset but almost no
sustained harmonic energy; a **buzzing** note carries broadband noise between
the partials; a **choked** note decays far faster than the exercise's expected
duration.

### `practice-eval` — scoring

Pure: in are `PlayedEvent`s and an `Exercise` plus a `Rubric`, out is an
`Evaluation`. No audio, no I/O, no clock of its own — testable with synthetic
sequences.

```rust
pub struct PlayedEvent {
    pub at_ms: f64,
    pub detected: Detected,
    pub confidence: f32,
    pub cleanliness: f32,
}

pub struct Evaluation {
    pub per_event: Vec<EventVerdict>,
    pub pitch_score: f32,
    pub timing_score: f32,
    pub cleanliness_score: f32,
    pub tempo_score: f32,
    pub total: f32,
    pub passed: bool,
}

pub enum EventVerdict {
    Hit { timing_error_ms: f32, cleanliness: f32 },
    WrongPitch { expected: Target, played: Detected },
    Late { by_ms: f32 },
    Early { by_ms: f32 },
    Missed,
    Extra,          // played something the exercise did not ask for
}
```

**Alignment — clock scores, elastic recovers.** Every expected event has an
instant on the metronome grid and a tolerance window; the millisecond deviation
*is* the timing error, so the clock is what produces the score. When the take
drifts past recovery — the player loses the place, stops, restarts a bar late —
a DTW pass re-finds the position so the remainder is still scored, instead of
becoming a wall of `Missed`. The elastic pass never *improves* a timing score:
it only decides which expected event a played event belongs to. That separation
is deliberate; an aligner that both matches and scores would happily stretch
time until every take looked punctual.

### Audio path

The evaluator subscribes to the chain's `InputTap`
(`crates/engine/src/input_tap.rs`) exactly as the tuner and the spectrum
analyzer do: the audio thread pushes samples into a lock-free SPSC ring inside
`process_input_f32`, and a worker thread polls the ring and runs every bit of
the DSP above.

- **Zero new lines in the audio thread.** No allocation, no lock, no I/O on the
  RT path (invariant #8).
- **No stream touches another** (invariant #4): the evaluator reads one chain's
  input tap, and nothing else.
- **The dry signal, before the block graph.** Distortion, delay, modulation and
  compression would wreck onset and pitch detection. The player hears their own
  tone through the rig while the evaluator listens to the clean guitar.
- Ring overflow drops samples rather than blocking, per the tap's existing
  contract; a dropped frame costs one detection round, never an xrun.

### Commands

Every state change is a `Command` (#127):

```
PracticeCommand::StartExercise { exercise: ExerciseId, bpm: Option<u32> }
              ::StopExercise
              ::RetryExercise
              ::SetStudentRubric { patch: RubricPatch }
              ::SetTargetBpm { bpm: u32 }
              ::MarkLessonSkipped { lesson: LessonId }
```

Read side: `openrig://practice` serves the live attempt — current event, running
scores, verdicts so far — so an MCP client sees what the screen sees.

### Persistence (ADR 0003)

- **System** (`config.yaml`): the student's own rubric overrides and the courses
  library path. These belong to the person and the machine, not to a rig.
- **Course/lesson data**: inside the course package, which the shell spec
  defines. Not in the `.openrig`.
- **Progress** (per lesson: status, best score, attempts, last attempt date):
  system scope, next to the student's overrides. A rig sent to another machine
  does not carry someone's grades.

Lesson status is an enum, not a boolean: `NotAttempted`, `InTraining`,
`NeedsWork`, `Mastered`, `Skipped`. The default flow lets the student move on
while carrying `NeedsWork`, which is what turns "am I ready?" into something the
app can answer later — *go back to lesson 4*.

## Testing

**`practice-eval` against synthetic sequences** — no audio involved:

- a perfect take → `passed`, every verdict `Hit`
- one wrong fret → exactly one `WrongPitch`, pitch score down, timing untouched
- one note 80 ms late with a 50 ms tolerance → one `Late`, timing score down
- a take that loses the place at bar 3 and restarts → the events after the
  recovery are scored, not a wall of `Missed`
- a take at half the target BPM → tempo score fails while pitch stays perfect
- an axis set to `None` in the rubric → that axis does not affect the total

**DSP modules against generated and recorded fixtures** — a synthesized clean
note, a recorded muted note, a recorded buzzing note, a chord with one wrong
finger, an onset burst at a known instant (assert the detected instant within a
few milliseconds).

**Rubric resolution** — course-only; lesson override; student override when
allowed; student override rejected when not allowed.

**No test may depend on the owner playing anything by hand.** Fixtures are
recorded once, committed, and reused (see
[[feedback_never_ask_user_to_manually_test]]).

Real-hardware verification of the full input path goes behind
`OPENRIG_HW_TESTS=1` per `docs/testing.md`.

## Risks

| Risk | Mitigation |
|---|---|
| Onset detection on a clean electric guitar with soft picking | Threshold is per-exercise data, not a constant; fixtures cover soft and hard attacks |
| Chord verification false-positives on close voicings | Score against the plausible *wrong* chords too, not only the target; report confidence and let the rubric set the bar |
| A frustrating evaluator (too strict) is worse than none | Defaults are generous, every axis is switchable off, and the student can loosen when the course allows |
| Latency between playing and feedback | Detection runs on a worker at the tap's poll rate; feedback per event is post-hoc, not real-time overlay |
| Scope creep into the course shell | The shell is a separate spec and a separate issue; this crate never imports a video or a lesson package |

## Open for the shell spec

Video playback inside the app (Slint has official
[GStreamer](https://github.com/slint-ui/slint/blob/master/examples/gstreamer-player/README.md)
and [FFmpeg](https://github.com/slint-ui/slint/blob/master/examples/ffmpeg/README.md)
examples — both bring a C library into a build pipeline that is pure Rust
today), lesson packaging, the path UI, and how a lesson arms the rest of the rig
(metronome BPM, chain preset, looper, score).
