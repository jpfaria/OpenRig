# #980 — xrun LED blinking: the dsp-worker delivers buffers late

Status: **FIX SHIPPED, awaiting the owner's ear** — cause located (H14,
memory pressure); the hardware test is green 4/4 with the fix.
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
  (`<wall>us wall / <cpu>us cpu (period, backlog)`), every RT policy
  re-declare (`dsp-worker realtime promotion`), every burst of process page
  faults (`task events 10ms: faults +N`) and every memory wiring pass
  (`memory residency: wired …`).
- Memory pressure matters: check `sysctl vm.swapusage` before a run. With the
  swap nearly empty the kernel reclaims nothing and the bug does not show.
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
| H7 | The same DSP runs 2-10x slower at times (slower core / memory stalls) | CONFIRMED (mechanism) | `tests/issue_980_chain_cost_profile.rs`, the owner's chain alone on one thread, no device, per-buffer instructions / cycles / P-core share from `proc_pid_rusage` (2026-09-28, 7 runs): median buffer (2 outputs) = 7.05 M instructions, 1.14 M cycles, 300 µs, 100% on a performance core, IPC 6.2, 3.8 GHz. Slowest buffers (0.6-2.6 ms, clustered, at DIFFERENT positions every run) = the SAME ~7.1 M instructions in 2-3x the cycles — on an efficiency core (0-46% of cycles on P, 2.6-2.8 GHz, IPC 2.1-2.7) or on a performance core with IPC 2.5-4 (memory stalls). One output alone: p50 148 µs. Per block: NAM 117 µs, IR 4, Supermassive 6, CloudReverb 14-18 (no block has bursty work). The chain never does extra work; macOS sometimes runs the same work 3-8x slower. |
| H7b | During the live late bursts the whole process ran on E-cores / down-clocked | REFUTED (process level) | Hardware run with `rusage` every 100 ms: at the idle-phase bursts 80-99% of the process's CPU was on P-cores at 3.5-3.7 GHz, IPC 3.5-5. The slowdown is per-thread (the worker's own buffers), not the whole process. |
| H12 | A block (VST3 internal re-blocking, denormals) does periodic heavy work | REFUTED | Cost profile: slow buffers execute ≤ 1.6x the median instructions (usually ≤ 1.05x); clusters move between runs; each block alone has no bursts. Pinned by `issue_980_chain_cost_profile.rs` (slow buffers ≤ 2x median instructions). |
| H8 | The BudgetTracker's RT re-declares (`thread_policy_set` every 3-15 s) perturb the worker | PARTLY — not sufficient | Late clusters often follow a re-declare by 2-3 s. A/B with re-declares off (local experiment, reverted): still lost audio in 2/4 rounds; fewer CPU-over-period late buffers in some rounds. |
| H9 | The worker is preempted mid-buffer | OPEN, evidence for | Trace: late buffers with wall ≫ CPU (e.g. 10.3 ms wall / 3.1 ms CPU, 3.1 ms / 0.9 ms). |
| H10 | The kernel demotes the worker out of the real-time band | REFUTED | Mach watch of both `dsp-worker` threads every 1 ms for ~70 s (`tests/worker_thread_watch`): `curpri` 97, base 97, policy 2 (time constraint) in all 69 036 samples, including across the late bursts (2026-09-28 16:03-16:05). |
| H11 | The BudgetTracker shrinks the declared computation below what bursts need | NOT THE CAUSE (see H14) | Same watch: declared computation starts at 1233 µs (85% of 1451), shrinks to 870-967 µs after ~12 s and to 725-737 µs after ~24 s; the late burst at 16:04:21 (CPU 2.3-2.7 ms, wall up to 13 ms) came 14 s after the drop to 725 µs. With the process memory wired (H14) the same tracker ran and nothing was late. |
| H7-live | Inside the app the dsp-worker runs on efficiency cores or down-clocked | REFUTED | Per-buffer `thread_selfcounts` in the live worker (throwaway probe, 2026-09-28 19:12): 100% of every buffer's cycles on performance cores at 3.3-3.6 GHz, 0 µs on efficiency cores. Normal buffer: 7.04 M instructions, 1.2-1.4 M cycles, IPC 5.2-5.9, ~330 µs. Late buffers: the SAME cores and clock, but 7.6-10 M instructions (+8..43%: kernel work) at IPC 0.6-2.3 — e.g. 4066 µs CPU = 8.86 M instructions in 14.5 M cycles, IPC 0.61. |
| H13 | Joining the DEVICE's audio workgroup is wrong for an asynchronous worker (Apple, `AudioWorkInterval.h`, case 3: own `AudioWorkIntervalCreate` interval) and makes the scheduler run it slow | REFUTED as the cause | Throwaway A/B, worker on its own work interval (start/finish per buffer, deadline = one period) vs the device join, same binary, alternating: underrun frames 0 / 512 / 768 / 14784 vs 128 / 384 / 512 / 46208; late buffers 20 vs 28 in a traced round, most of them wall ≫ CPU in both. Apple's guidance still applies to the design; it is not what makes the worker late. |
| H14 | Memory pressure: the kernel compresses pages of the process that the DSP touches (reverb delay lines are touched once per loop, seconds apart), and the worker stalls decompressing them | CONFIRMED | Owner's machine: 18 GB RAM, swap 11.6-14.3 GB used of 12-14 GB during the runs, other agent sessions compiling. (1) Every late buffer (8/8) sat within 30 ms of a burst of 40-160 process page faults in 10 ms (`task_info TASK_EVENTS_INFO`, `pageins` 0 = compressor, not disk). (2) A/B on the hardware test (throwaway, both with the #979 slack), `mlock` of every private writable region after the chain starts (~1.17 GB, 269 regions, 0 refused) vs not, alternating: mlock 0 / 0 / 0 frames lost, 4 / 0 / 12 late buffers, 75 / 16 / 37 extra faults; plain 10176 / 50816 / 0 frames lost, 142 / 347 / 0 late, 68312 / 150374 / 98 extra faults — the plain round that lost nothing was the one where the kernel reclaimed nothing. What gets wired: ~850 MB of `MALLOC_SMALL` regions (~800 MB resident) — the plugins' and models' memory; the rest is small. |

## Shipped

| Commit | Change | Pinned by |
|---|---|---|
| `552a21080` | Drift guard never trims a primed route below its prime (H1 — correct but not the cause) | `issue_980_a_primed_route_is_never_trimmed_below_its_prime`, `issue_980_a_primed_route_keeps_its_cushion_after_a_lucky_window` (mutation-checked) |
| `780b9f44e` | Opening/saving a project writes only `recent_projects`, no longer the whole stale config (the `syn2-main` `[3]`→`[7]` reversion) | `crates/application/tests/issue_980_recents_save_keeps_io_bindings.rs` |
| `1c14b1fad` | `openrig://routes` gains `dropped_frames` and `input_busy_skips` — what made the late-worker signature visible | `crates/engine/src/issue_980_route_loss_counters_tests.rs` |
| `def1a9fde` | Hardware RED test reproducing the measured symptom | itself |
| this commit | macOS: a `memory-residency` thread wires the process's private writable memory (`mlock`, touched regions only, ≤ 256 MB each, ≤ ¼ of RAM, each once) when the engine starts, the moment a runtime goes live (`LiveRuntimeSlot::new` / `publish`) and every 5 s (H14; owner approved wiring ~1.2 GB on 2026-09-28). No latency change. | `memory_wiring_tests.rs` (touched buffer wired, wired once, concurrent scans wire once, untouched reservation never wired), `memory_residency_keeper_tests.rs` (later allocations wired, mutation-checked), `tests/issue_980_engine_keeps_audio_memory_resident.rs`, `tests/issue_980_live_runtime_memory_wired_at_once.rs`, and the hardware test green 4/4 |

| follow-up to #981 (this branch, second PR) | After an adversarial review of #981 (below): passes count every wired region against the budget and run one at a time with a re-check before each `mlock`; memory left unwired is logged; the keeper is woken by in-place live edits too (VST3 branch, resync keeping the streams); a follow-up pass 1 s after each wake catches buffers allocated zeroed; the console / headless path starts the keeper. | `memory_wiring_pass_tests.rs`, `memory_wiring_report_tests.rs`, `issue_980_in_place_edit_wires_memory_tests.rs`, `tests/issue_980_zeroed_buffer_wired_once_written.rs`, `tests/issue_980_console_streams_keep_memory_resident.rs`; the existing wire-once tests went red (11 wires on one region) when the pass snapshot widened the race, green with the lock |

Parked, not shipped: per-buffer worker timing counters in the MCP
(`git stash` on the solver, "980 worker timing WIP") — wiring untested.

## Fixes tried on the hardware test (2026-09-28, throwaway clones)

Frames lost per 90 s run (4 routes), alternating runs, load 7-22:

| Variant | Runs | Frames lost |
|---|---|---|
| `bug/issue-980` as is | 3 | 7040, 7296, 9344 |
| + #979 one-buffer slack (owner-approved latency) | 7 | 8256, 896, 0, 6016, 384, 20096, 1856 |
| + #979 slack, Main output only (one pipeline per worker, as an approximation of H6) | 4 | 0, 0, 704, 832 |
| + #979 slack, worker on its own work interval (H13) | 4 | 0, 512, 768, 14784 |
| + #979 slack, process memory wired (H14) | 3 | 0, 0, 0 |
| `bug/issue-980` as is, process memory wired (no slack, no extra latency) | 3 | 0, 0, 0 |
| `bug/issue-980` as is, same batch, not wired | 3 | 1280, 19200, 1152 |
| Production keeper, periodic 5 s passes only | 3 | 512, 0, 0 |
| `bug/issue-980` as is, same batch | 3 | 2432, 24832, 1792 |
| Production keeper + wake when a runtime goes live (shipped) | 4 | 0, 0, 0, 0 |

The slack and the split only make the worker's stalls cheaper; wiring the
memory removes the stalls, with no latency added. With periodic passes only,
every late buffer of the failing run fell between the chain coming up and the
pass that wired its 655 MB 5 s later: the kernel compressed the new chain's
pages within those seconds. Waking the keeper when a runtime goes live wires
it ~0.5 s after start-up; the only late buffers left are the two of the cold
start, before the idle measurement (swap 13.4 GB used, load ~11).

## Review of the fix (#981, 2026-09-28)

Read-only review, 4 reviewers (Mach calls, memory lifecycle, real-time
safety, platform/CI), each finding checked by a skeptic told to refute it.

| Finding | Verdict | Outcome |
|---|---|---|
| Budget checked against a running total in address order: a region below already-wired ones could be wired past ¼ of RAM | CONFIRMED | Fixed: every wired region counts first |
| Memory left unwired (budget, > 256 MB, `mlock` error) was silent | CONFIRMED | Fixed: warning in the log when it changes |
| Wire-count tests race the keeper; nothing serialises passes (`mlock` stacks a wire per call) | CONFIRMED | Fixed: one pass at a time + re-check before `mlock` |
| In-place live edits (VST3 branch, resync keeping the streams) never wake the keeper — up to 5 s unwired | CONFIRMED | Fixed: both wake it |
| A new runtime's zeroed buffers (no VM object yet) are skipped by the wake-up pass | CONFIRMED | Fixed: follow-up pass 1 s after each wake |
| Console / headless rig never start the keeper | CONFIRMED | Fixed: `build_streams_for_project` starts it |
| Freed heap in a wired region stays resident (footprint follows the peak); a region with one touched page is wired whole | CONFIRMED | Kept, documented as the cost of wiring whole regions |
| CI never compiles the macOS wiring code | REFUTED | — |
| Priority inversion between the keeper and the dsp-worker | REFUTED | — |
| Every pass walks every page of every region | REFUTED | — |

## Why it happens (as of 2026-09-28)

The dsp-worker of each guitar runs BOTH pipelines of that guitar (Main and
Out 2) one after the other: ~0.33 ms per buffer on a performance core at
3.6-3.9 GHz (7 M instructions, IPC ~5.5). The owner's machine runs out of
memory (18 GB, the swap full while other agent sessions compile), and the
kernel compresses pages of OpenRig that are not touched every buffer — the
reverbs' delay lines are walked once per loop, seconds apart. When the worker
reaches such a page it stops until the kernel decompresses it: the buffer
takes 1-4 ms of CPU (IPC down to 0.6, +8..43% kernel instructions) or waits
off-CPU, crosses the 1.451 ms period, the output plays silence and the
catch-up is dropped (`underruns == dropped_frames`) (H14). With the process
memory wired nothing is late. Two pipelines per worker (H6) and the lean
64-frame cushion (#979) only make each stall more likely to cost audio.

## Open

- The owner's ear on his rig with the fix (the hardware test is silent).
  First live reading, 2026-09-28 18:06-18:08, app built from `405ede3d8`'s
  tree, owner playing guitar 1 (peak -1..-8 dBFS) with the swap 14.1 GB used:
  over ~86 700 callbacks (~125 s) every route +0 underruns, +0 dropped
  frames. The owner: "parece que está muito melhor".
- 18:06-18:07 the owner switched both VST3 off and on again live: no new
  underrun or dropped frame, and the streams were not recreated (callback
  counters kept counting). 18:10: still +0 / +0 since 18:05. The app (with
  its UI) wired 1491 MB at start-up, 1526 MB after the edits.
- Same readings: the routes' `fill_frames` alternates 128 / 64 (18 of 30
  fast reads 128, 11 read 64) — a route holds 128 frames (its whole ring:
  target 64, capacity 128) at the callback start and 64 after the pop, one
  buffer (1.45 ms) later than the route's design, with no room for an extra
  buffer. The routes had dropped 5056 / 7424 / 10240 / 12672 frames
  before the reading (none during it, no underruns: overflow, not silence) —
  most likely while the app started or the chain was rebuilt. Not caused by
  the memory fix (it does not touch the rings); matches #979's "route born
  after its producer ran ahead rests one buffer later for good". To measure
  from a cold start.
- Linux (JACK / Orange Pi) and Windows keep no memory resident — not measured
  there.
- One dsp-worker per pipeline (plan
  `docs/superpowers/plans/2026-09-27-issue-980-pipeline-per-worker.md`) is not
  needed for this symptom: with the memory wired, two pipelines per worker
  lost nothing.
- Enabling a VST3 from the GUI recreates every stream (~1.7 s of silence).
- Once in 8 hardware runs the chain never started streaming within 30 s.
- A passing hardware run ended with the test process crashing (SIGSEGV)
  after `test result: ok` — at process exit, after the chain with two VST3s
  was torn down (2026-09-28 16:1x). Not investigated.

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
