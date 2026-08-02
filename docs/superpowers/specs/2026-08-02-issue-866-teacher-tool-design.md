# Teacher tool (#866, spec 5 of 5)

**Status:** design approved (owner, 2026-08-02); spec under review.

**Series:** spec 5 of #866. Extends the **OpenRig Creator** of #309 — the same
desktop tool that packages plugins packages courses. One tool, two item types.

## Problem

Everything the platform promises depends on a teacher being able to produce a
lesson without becoming a software engineer. A lesson is not just a video: it
carries an exercise, a target the student is measured against, a rig setup, and
a rubric. Authoring that by hand in YAML is possible — the format is deliberately
human-writable — but it is not how a guitar teacher will spend their afternoon.

## Decisions taken (owner, 2026-08-02)

- **The video comes in finished.** The teacher records it however they like and
  uploads the file. The tool does not become recording software.
- **The exercise target is captured by playing it.** The teacher plays the
  passage and the tool captures it; what they played becomes what the student is
  measured against.

That second decision is the interesting one, and it is what makes the tool worth
building.

## The hard part, stated honestly

Spec 1 avoids blind transcription: the evaluator always knows what was expected,
so it verifies rather than transcribes. **The teacher tool does not have that
luxury.** Turning a recorded performance into a note-by-note target *is* the
transcription problem, and it is genuinely hard — especially polyphonically.

So the tool does not pretend to solve it. It splits the job:

```
teacher plays ──► capture (dry, via InputTap) ──► rough transcription
                                                        │
                                                        ▼
                                            teacher corrects in the tab editor
                                                   (#864 phase 3)
                                                        │
                                                        ▼
                                              ExpectedEvent[] — the target
```

1. **Capture** reuses the evaluator's front end: the dry input tap, onset
   detection, and YIN pitch detection. Monophonic passages — riffs, scales,
   arpeggios, the overwhelming majority of exercise material — transcribe well.
2. **Rough transcription** produces notes with timing, quantised against the
   metronome the teacher played to. It will be wrong sometimes, and the tool
   says so rather than hiding it: low-confidence events are flagged.
3. **Correction** happens in the score editor from #864 phase 3. This is the
   same editor, not a second one — which is a strong argument for #864's editor
   phase being on the critical path of the course platform, not a nice-to-have.
4. For chords, the teacher names the chord and the tool takes it; asking a
   polyphonic transcriber to guess a voicing is worse than asking a guitarist to
   type "Am".

**The target can also skip capture entirely:** import a range of an existing
`.gp` file (#864) and it becomes the target directly. Capture is the convenient
path, not the only one.

## What the tool does

### Course structure

Create a course, add modules, add lessons, reorder, set metadata (title, author,
level, language, licence). Set the course-wide rubric — the default standard
every lesson inherits (spec 1).

### Per lesson

- **Video**: pick the file, upload, watch the transcode status (spec 3), or
  paste a YouTube id for the cheap path (spec 2).
- **Text**: markdown, what the teacher wants to say in writing.
- **Rig setup**: tempo, time signature, count-in, chain preset, tuning, the
  score and the passage it drills, loop region, DI file. The tool sets these
  **by using OpenRig itself** — the teacher builds the tone in the normal chain
  editor and the tool captures it, rather than reimplementing a preset editor.
- **Exercise**: capture or import the target, correct it, then set the
  lesson-level rubric overrides — tolerances, target BPM, pass mark, whether it
  blocks, and whether the student may loosen it.
- **Preview as a student**: open the lesson exactly as a student sees it, play
  the exercise, and see the verdicts. A teacher who cannot pass their own rubric
  has set the rubric wrong, and finding that out before publishing is the single
  most useful thing this tool can offer.

### Publishing

Validate the package, upload it and its videos, submit a version. Semver, and a
published version is immutable — a fix is a new version (#309).

Validation catches what a student would hit: a lesson with no video, an exercise
with no target, a preset referencing a plugin the course does not declare, a
rubric that nothing can pass, a blocking gate on a lesson whose exercise is
empty.

## Where it lives

Inside OpenRig, behind a **teacher mode**, rather than as a separate application.

The reasons are concrete: the tool needs the audio input, the chain editor, the
metronome, the tuner, the score editor and the evaluator — that is most of
OpenRig. Rebuilding any of it in a second app would fork the code that matters
most. #309 imagined a separate Creator for plugins; for courses the dependency
on the live rig is total, and the same argument probably applies to plugins once
they need to be auditioned.

Teacher mode is off by default and enables the authoring surfaces. It is not a
separate binary, a separate build, or a separate repository.

## Testing

- Capture → transcription against recorded fixtures: a clean scale at 80 BPM, a
  riff with palm mutes, a passage with a wrong-confidence note that must be
  flagged rather than silently accepted.
- Package validation: every failure case above produces a specific message, not
  a generic "invalid".
- Round trip: build a course in the tool, install it as a student, and assert
  the lesson arms the rig exactly as authored.
- Preview-as-student uses the same code path as the student client — asserted by
  test, because a preview that diverges from reality is worse than no preview.

## Risks

| Risk | Mitigation |
|---|---|
| Transcription accuracy disappoints | The tool never claims to be right: low confidence is flagged, correction is expected, and importing a `.gp` skips capture entirely |
| It depends on #864 phase 3 (the editor) | Correction needs an editor; without it the tool ships with import-and-adjust only, which is usable but weaker. This dependency should drive #864's phasing |
| Teacher mode complicates the app for players | Off by default, and the surfaces only appear when enabled |
| Authoring is still too much work | The preview-as-student loop is where that shows up first; measure how long a real lesson takes to author before opening the tool to other teachers |
