# #980 — xrun LED blinking: the dsp-worker delivers buffers late

Status: **OPEN** — reproduced on the real interface; cause partly located.
Issue: https://github.com/jpfaria/OpenRig/issues/980 · Branch: `bug/issue-980`

## Symptom (reported)

- 2026-09-24: "xrun direto quando habilitei o VST3" — the xrun LED blinks as
  soon as two VST3 reverbs are enabled on the ANAL+DIG chain; "uma coisa
  horrível", "não para de piscar o aviso".
- The same session also hit "no sound on the FRFR / SYN-2" — that was NOT this
  bug (see *Not this incident* below).

## Symptom (measured)

`openrig://routes`, owner's rig, installed release 0.5.1 and later the solver
dev build:

| When | Load | Reading |
|---|---|---|
| 24/09 17:2x, release 0.5.1, VST3 just enabled | 11-14 (other sessions compiling) | ~7k callbacks: group0 +832 underrun frames per route, group1 +3072 / +4544 |
| 24/09, release, NAM + cab + 2 VST3, insert off | ~4.4 | 0 new underruns over ~6k callbacks (steady) |
| 24/09, release, enabling a VST3 from the GUI | ~4.4 | +1152 underrun frames per route in one burst, `fill_frames 0`, all routes silent: the enable recreated every stream (callback counters reset, new dsp-worker threads) |
| 25/09 05:0x, dev build `1c14b1fad`, 20 s delta | 7-10 | group0 `[0,1]` and `[10,11]`: underruns +3904, dropped +3904, busy 0; group1: +2944 / +2944 / 0 |

`underruns` counts FRAMES (one per missing sample frame), not callbacks — an
early reading of "27% of callbacks" compared frames with callbacks and was
wrong (~0.4% of frames).

**Signature:** `underruns == dropped_frames` exactly on every route,
`input_busy_skips == 0`, bursts of 128..1536 frames. That means the
dsp-worker delivered its buffers LATE: the output played silence, and when the
worker caught up the surplus hit a full ring and was dropped.

## Rig

- PreSonus Quantum HD 8, 44.1 kHz, 64 frames (1.451 ms period), macOS.
- Chain ANAL+DIG (`rig:input-7`): E/S `guitarra-1` (In 1, `[0]`) and
  `guitarra-2` (In 2, `[1]`), each E/S with two stereo outputs: Main `[0,1]`
  and "Out 2" `[10,11]`.
- Blocks: Insert `syn2-main` (bound, OFF in the measured runs), NAM
  `nam_synergy_dumble_os_a2`, IR `ir_cel_cream_4x12`, VST3
  ValhallaSupermassive, VST3 CloudReverb (owner's parameters in the fixture
  below).
- Topology as built: 2 per-E/S runtimes, each running 2 full chain copies
  (one per output) back to back on ONE dsp-worker: 4 pipelines, 2 workers,
  8 VST3 instances.
- Route cushion on this rig: every route is same-device, target 64, prime 64
  (convolver-fed), ring capacity 128 — one buffer of margin.

## Reproduce

```sh
OPENRIG_HW_TESTS=1 \
OPENRIG_OWNER_PLUGINS=<OpenRig-plugins/plugins/source> \
OPENRIG_980_TRACE=1 \
cargo test -p infra-cpal --release \
  --test issue_980_owners_two_guitars_two_outputs -- --nocapture
```

- Test: `crates/infra-cpal/tests/issue_980_owners_two_guitars_two_outputs.rs`
  (fixture `crates/engine/tests/fixtures/presets/issue_980_owner_anal_dig.yaml`).
  Macos + release + the Quantum connected; silent (chain volume 0).
- `OPENRIG_980_TRACE=1` prints every late dsp-worker buffer
  (`<wall>us wall / <cpu>us cpu (period, backlog)`) and every RT policy
  re-declare (`dsp-worker realtime promotion`).
- RED on 2026-09-28, idle machine (load ~4): idle 60 s — group0 (576, 576, 0)
  per route, group1 (960, 960, 0); 30 s with 12 spinning threads — (64, 64, 0)
  per route. Same signature as the live rig. It is intermittent: later runs
  at load 7-10 lost 0 to 2112 frames per route per minute.

## Hypotheses

| # | Hypothesis | Verdict | Evidence |
|---|---|---|---|
| H1 | The #953 drift guard trims a primed route to a single buffer | REFUTED as the cause | Guard can never fire on a same-device 64-frame route (floor would have to exceed the 128-frame capacity). `552a21080` shipped anyway (correct, harmless) and the rig kept underrunning. |
| H2 | A non-RT thread holds the runtime's `processing` lock (lost `try_lock`) | REFUTED | `input_busy_skips` stayed 0 in every live and hardware reading. |
| H3 | Two OpenRig instances on the same interface | REFUTED as the cause | Seen once (an old instance held the MCP port); the underruns continued with a single instance. |
| H4 | The CPU cost of the chain exceeds the period on average | REFUTED | `sample`: each dsp-worker ~35-45% busy, ~55-65% asleep; normal DSP 0.5-0.8 ms CPU per buffer for both pipelines. |
| H5 | The config reversion (`syn2-main` `[3]` → `[7]`) causes the underruns | REFUTED | Underruns continued with `[3]` persisted; the reversion was a separate bug (fixed, see Shipped). |
| H6 | Two pipelines of one guitar on ONE dsp-worker make it late | PARTLY CONFIRMED | Hardware probe, idle 60 s, 3 rounds each: both outputs lost audio in 3/3 rounds (up to 1344 frames/route); Main only (one pipeline per worker) in 1/3. Contract test `crates/engine/tests/issue_980_one_worker_per_pipeline.rs` (1 input × 2 outputs must build 2 runtimes) is RED today — kept uncommitted until the fix lands. |
| H7 | The same DSP runs 2-10x slower at times (slower core / lower clock) | OPEN, evidence for | Trace: late buffers with CPU time 1.4-6.5 ms against the 0.5-0.8 ms norm, CPU ≈ wall. Loss is often LOWER with 12 spinning threads than idle (keeps cores awake). Core type not measured yet. |
| H8 | The BudgetTracker's RT re-declares (`thread_policy_set` every 3-15 s) perturb the worker | PARTLY — not sufficient | Late clusters often follow a re-declare by 2-3 s. A/B with re-declares off (local experiment, reverted): still lost audio in 2/4 rounds; fewer CPU-over-period late buffers in some rounds. |
| H9 | The worker is preempted mid-buffer | OPEN, evidence for | Trace: late buffers with wall ≫ CPU (e.g. 10.3 ms wall / 3.1 ms CPU, 3.1 ms / 0.9 ms). |

## Shipped

| Commit | Change | Pinned by |
|---|---|---|
| `552a21080` | Drift guard never trims a primed route below its prime (H1 — correct but not the cause) | `issue_980_a_primed_route_is_never_trimmed_below_its_prime`, `issue_980_a_primed_route_keeps_its_cushion_after_a_lucky_window` (mutation-checked) |
| `780b9f44e` | Opening/saving a project writes only `recent_projects`, no longer the whole stale config (the `syn2-main` `[3]`→`[7]` reversion) | `crates/application/tests/issue_980_recents_save_keeps_io_bindings.rs` |
| `1c14b1fad` | `openrig://routes` gains `dropped_frames` and `input_busy_skips` — what made the late-worker signature visible | `crates/engine/src/issue_980_route_loss_counters_tests.rs` |
| `def1a9fde` | Hardware RED test reproducing the measured symptom | itself |

Parked, not shipped: per-buffer worker timing counters in the MCP
(`git stash` on the solver, "980 worker timing WIP") — wiring untested.

## Open

- Why the same DSP work takes 2-10x longer at times (H7): measure the core
  type / clock of the late buffers.
- Whether one dsp-worker per pipeline (plan
  `docs/superpowers/plans/2026-09-27-issue-980-pipeline-per-worker.md`,
  owner decisions: fixed slots across the insert switch, JACK included)
  brings the hardware test to zero — H6 says it helps, not that it is enough.
- Enabling a VST3 from the GUI recreates every stream (~1.7 s of silence).
- Once in 8 hardware runs the chain never started streaming within 30 s.

## Not this incident

- "No sound on the FRFR": the HD 8 FRFR bus fader (`aux/ch10`, SMC-Mixer
  fader 2) was at -inf. Checklist: `music-setup/docs/troubleshooting.md`.
- "No sound through the SYN-2": `syn2-main` send on `[7]` (USB 8), which the
  `MIXER-ON` scene disables; correct is `[3]`. Root cause of the recurrence
  fixed in `780b9f44e`.

## Related

- #979 — insert return stacking; also shipped drift-guard changes that went
  green while the rig kept underrunning.
- #670 (dsp-worker off the HAL thread; NAM cold-cache tail ~1.4 ms), #698
  (adaptive RT budget), #760 (workgroup join per device), #781 (worker sleeps
  instead of spinning), #953 / #965 / #969 (route cushion and drift guard).
