# Audio incidents — history

Every problem that changed what the owner hears (dropouts, clicks, xrun LED,
silence, level, latency, a doubled or thin sound) gets one file here. The file
records what was reported, what was MEASURED, how to reproduce it, every
hypothesis that was tested — including the ones that were wrong — and what was
shipped to fix it, with the tests that pin the fix.

**Why this exists:** the same symptoms come back. Without this history each
new session re-derives causes that were already measured, re-tries fixes that
were already refuted, and trusts a green test that never reproduced the rig's
numbers — #979 and #980 each shipped drift-guard changes whose tests went
green while the owner's rig kept underrunning.

## Rules for agents

1. **Before diagnosing any audio problem**, search this folder for the
   symptom and the counter signature you see, e.g.
   `grep -ril "underruns == dropped" docs/audio-incidents/` or the chain
   shape (`two outputs`, `insert`, `VST3`, `NAM`). Read every match first.
2. **A hypothesis marked REFUTED here is not re-tested** unless the new
   evidence says why the old refutation does not apply.
3. **Every audio diagnosis or fix updates this folder in the same commit**:
   a new file for a new incident, or a new dated section in the existing one.
   Record numbers exactly as measured (counter, value, duration, load), never
   paraphrased.
4. **A fix is only "done" when the incident file names the test that
   reproduces the measured symptom** (real topology, real counters — see
   `docs/testing.md` → "Real-hardware battery") and that test is green.

## File format

`NNN-short-slug.md`, where `NNN` is the GitHub issue. Sections, in order:

- **Symptom (reported)** — the owner's words and when.
- **Symptom (measured)** — counters from `openrig://routes` / meters / logs,
  with duration and machine load.
- **Rig** — interface, rate, buffer, E/S, outputs, inserts, blocks.
- **Reproduce** — the test file(s) and the exact command.
- **Hypotheses** — one line each: `CONFIRMED`, `REFUTED` or `OPEN`, with the
  evidence (numbers or test) that decided it.
- **Shipped** — commits, what each changed, the test that pins it.
- **Open** — what is still unexplained.
- **Related** — other incidents with the same signature.

## Index

| Incident | Signature | Status |
|---|---|---|
| [980](980-dsp-worker-late-underruns.md) | xrun LED blinking; `underruns == dropped_frames` on every route, `input_busy_skips 0`; bursts of 128..1536 frames; NAM + IR + 2 VST3, 2 guitars x 2 outputs, 44.1 kHz / 64 | OPEN — reproduced on hardware, cause partly located |
