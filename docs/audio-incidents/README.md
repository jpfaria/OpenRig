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
5. **If the symptom is happening RIGHT NOW, capture before you think.** An
   intermittent bug gives one chance per occurrence. When the owner says "tá
   acontecendo" / "continua o problema", the very first action is a capture of
   the broken state while it lasts — `tools/rec` on the HD 8 inputs,
   `openrig://routes` + `meters`, `sample <pid>` — in one parallel batch, before
   any analysis, issue or answer. Better: start a rolling recorder plus a 0.5 s
   MCP poller (one that survives MCP timeouts) as soon as an intermittent bug is
   first reported, so the window is already being recorded. #979 (2026-09-24):
   the window was spent reading one route snapshot and writing the issue, he
   toggled the chain to clear it, and the broken state was never recorded —
   "vc tinha que ter medido qdo te mandei o problema… agora nao adianta".
6. **A symptom on the owner's machine can come from another agent session.**
   Several solver sessions run test suites on the same machine all day, and a
   test that touches per-machine state (`config.yaml`, presets) with env-based
   isolation can leak across an async boundary and overwrite his real files —
   the symptom then looks like an app bug. "Device settings lost on every open"
   (#701) was exactly this: the #693 branch moved config writes to an async
   persist worker, and `with_tmp_home` restored the real `$HOME` before the
   queued write landed, so every test run rewrote his real config with fixtures
   (`new_dev`, `dev_a`, `dev_b`). Before blaming a `develop` regression: read the
   real config (fixture-looking ids are the smoking gun), compare its mtime with
   `ps aux | grep cargo` — including hung test *binaries* that swapped `$HOME`
   to a FIFO and never exited — hash the file, run the suspected tests, re-hash,
   and restore his file from the snapshot immediately with a backup kept aside.
   Structurally fixed in #731: config writes bind the path at DISPATCH time via
   `application::app_config_persist::{persist_app_config,
   persist_app_config_snapshot}` (plus the `*_at(path)` variants in
   `crates/infra-filesystem/src/app_config_io.rs`), and `with_tmp_home` flushes
   the worker before restoring `$HOME`. Any NEW config write site goes through
   `app_config_persist`, never `persist_worker::run(|| save_app_config(...))`.

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
| [979](979-loop-and-stacked-sound.md) | "loops" / "several streams stacked" until the chain is restarted; insert on: `[0,1]` twice in `openrig://routes` (routes 0 and 2, fill 64 vs 128); `latency_trims` and `underruns` climbing in 64-frame steps with the workers ~21-28% busy; 2 guitars sharing Main, HD 8 44.1 kHz / 64 | FIX IN PROGRESS (branch) — return once per physical output, chain volume once, drift-guard spiral + one buffer of slack, no fade restart on an in-place edit |
| [980](980-dsp-worker-late-underruns.md) | xrun LED blinking; `underruns == dropped_frames` on every route, `input_busy_skips 0`; bursts of 128..1536 frames; NAM + IR + 2 VST3, 2 guitars x 2 outputs, 44.1 kHz / 64; the machine's swap full; late buffers on bursts of page faults | FIX SHIPPED (memory kept resident, macOS) — awaiting the owner's ear |
| [987](987-click-on-live-edit.md) | a small click on block on/off or scene switch; offline: step (second difference ~1000x the tone's) + short gap at the swap (fresh rebuild), or 2-3 callbacks of raw input while an in-place edit builds (chain holding a VST3) | FIX ON THE BRANCH — awaiting the owner's ear |
| [998](998-failed-edit-plays-dry.md) | the chain plays dry after a live edit; log `rebuild failed ... restoring previous state`; node serials `before=[1, 2] after=[]` on a `Select` with an unknown option | FIX ON THE BRANCH (unknown option); mixed-layout / failing-option select still open |
| [992](992-filter-silent-on-live-guitar.md) | a filter "does nothing" on the live guitar but "works" on the DI loop; true-stereo blocks swapped for a faulted bypass ("does not accept mono input") on a chain whose outputs are all mono (`processing=Mono`); single `openrig://di` `out_dbfs` peaks misread as a DI-only effect | FIX ON THE BRANCH — true-stereo on a mono bus is broadcast (`bus_layout_after`); the rest measured as expected behaviour |
