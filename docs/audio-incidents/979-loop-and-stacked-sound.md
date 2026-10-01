# #979 — the sound "loops" and sounds stacked until the chain is restarted

Status: **FIX IN PROGRESS on `bug/issue-979`** (now on release/v0.6.0) — the
2026-09-30 returns are the HD 8 input itself arriving stepped for every client
until the chain restarts (H27/H28); the automatic restart (fix 2) is on the
branch, waiting for the rig, and the workgroup fix (fix 1) is on hold until a hardware red
reproduces the step (see "code recon" below). The earlier insert fixes
(stacked sound, chain volume twice, scene-edit click) are on the branch.
Issue: https://github.com/jpfaria/OpenRig/issues/979 · Branch: `bug/issue-979`

## Symptom (reported)

- 2026-09-24, installed 0.5.1: playing chain `ANAL+DIG` (`rig:input-7`) with
  the `syn2-main` insert enabled, "the sound suddenly starts looping — it
  sounds like several streams stacked on top of each other". Switching the
  chain off and on fixes it; after a while it comes back on its own.
- Later the same week: the symptom came back after the fast chain-switch fix
  (#967); the dev build "underruns from the first second".

## Symptom (measured)

| When | Reading |
|---|---|
| 24/09, insert ON, symptom present | `openrig://routes`: one group, 5 routes — `[0,1]` twice (routes 0 and 2, **fill 64 vs 128**), `[24,25]`, `[10,11]`, `[3]` (the send). 1 CoreAudio IO thread, 1 dsp-worker. With guitar 2 muted (−57 dBFS) the return In 3 kept ringing at ~−10 dBFS with tonal content (196 / 291 / 657 Hz) and a ~610 ms self-similarity; bypassing the insert over MCP dropped In 3 to −40 dBFS. |
| 24/09 22:23–22:31, insert OFF (scene `ANAL+DIG`), 2 groups | 22:23:18 rebuild → 0 underruns, 0 trims; 22:31:01 first `latency_trims` on all 4 routes (fill 64); 22:31:13–22:31:25 underruns 0 → 256 (group 0) / 320 (group 1), trims 1 → 4, fill 0; then frozen at 256/320 while the sound still "looped". Both dsp-workers RT (pri 97), ~21% busy in NAM. |
| 40 min watch, no symptom reported | 4512 route reads: 0 underruns, 0 trims, fill ≤ 128. 3 rebuilds with nobody touching the rig (13.1, 18.1, 35.5 min); route `[3]` appears/disappears with them. |
| dev build | sibling routes of one stream: 12032 vs 2496 underrun frames. The offline quality probe read +10.1 dBFS peak on group 0 and `dsp_latency_ms` 8.48 → 15.62. |
| 29/09 14:56–15:10, symptom present (v0.5.2) | Splice signature: per-phase mean of the 3rd difference folded at 256 frames, max/median. OpenRig's output (CoreAudio process tap, `out6`/`out7`): 6.97 / 16.52 at one fixed phase; at that phase the sample after the boundary misses the slope by −0.48× (R² 0.94, other phases R² ≤ 0.48) — a sub-sample time jump every 256 frames, not a gain step and not a block replay. **The HD 8 INPUT recorded at the same time by a separate process (`music-setup/tools/rec`, straight from the driver) carries the same splice, same phase on every channel:** In 2 (guitar) 239 / 927 / 35, In 3/4 (the SYN-2 return) 10.8–15.9 (`live`, `live2`, `in4`, `in7`). From 15:14 on (`in8`…`in19`) every input reads ≤ 1.2 and the output 1.0–1.5. Offline `render_chain` of the live cab + CloudReverb on a sine: ≤ 1.8 at 64–512 (no splice). |
| 30/09 09:56–10:02, symptom present (dev build `feature/issue-1007`, started 09:25; HD 8 mixer ON, `Analog + ADAT`) | Rolling watch (HD 8 inputs recorded outside OpenRig every ~25 s): 1.1 on every input until 09:56:18; **09:56:44 In 1 = 39.4, In 3/4 = 5.5**, then 45.9 / 5.2 (09:58:56), 15.6 / 5.8, 15.0 / 5.7, 29.1 / 6.9 (10:02:47); **In 2 stayed 1.1–1.7 throughout**. The large third differences sit in 4-sample clusters on a 64-frame grid (e.g. frames 7156, 7796, 18932: all multiples of 64 apart), the same positions on In 1 and In 3. Routes log (every 2 s): `rig:input-7` 0 underruns / 0 trims from its 09:52:17 start; **09:56:20 the insert routes `[7]` and `[2]` go underrun 0 → 64, trims 0 → 1; 09:56:54 every route of the chain underrun → 128 (insert routes 256), trims → 2**; then frozen (no new underrun or trim) while the inputs kept the splice. HAL processor-overload listener on the HD 8 (from 10:00): no overload event. The owner toggled the chains at ~10:02–10:03: counters reset, every input 1.1 from 10:03:12. |

## Rig

- PreSonus Quantum HD 8, 44.1 kHz, 64 frames (1.451 ms), macOS.
- `io_binding_ids: [guitarra-1, guitarra-2]`: In 1 and In 2, mono. The issue
  body: both → Main `[0,1]`; `guitarra-1` also → `[24,25]`, `guitarra-2`
  also → `[10,11]`. The #980 fixture has both on Main + `[10,11]`.
- Insert `syn2-main`: send mono `[3]` (USB playback 4, the HD 8 mixer sends it
  to Out 8 → pedals → SYN-2), return stereo In 3/4 `[2,3]`.
- Blocks: insert, NAM `nam_synergy_dumble_os_a2`, IR `ir_cel_cream_4x12`, two
  VST3 reverbs.

## Reproduce

All engine-level, no hardware (the #979 two-head rig harness, real topology,
the HAL cycle as #965 measured it):

```sh
nice -n 19 cargo test -j 4 -p engine --lib -- issue_979_two_head_rig
nice -n 19 cargo test -j 4 -p infra-cpal --lib -- issue_979
```

- `crates/engine/src/issue_979_two_head_rig_tests.rs` and its children
  (`issue_979_*_tests.rs`: insert_return_stacking, volume_and_gain,
  issue_body_topology, decay_and_device_stall, late_worker_recovery,
  cushion_shedding_969, sibling_route_divergence, head_route_offset,
  frame_integrity, …).
- `crates/infra-cpal/src/issue_979_*_tests.rs` (one_period_late, slack_rest,
  output_write_through, stream_layer_replaced_runtime, device_notifications,
  stored_structure).

### Test cost (#991)

Under `cargo llvm-cov` these simulations ran long enough to time out the CI
Coverage job. #991 cut their cost without touching an assertion; each test
whose parameters were cut was re-run on the pre-#979 production
(`0060ead9e`, the current test tree on top) and is still RED, and GREEN on
the fix:

| Test | Cut | RED on `0060ead9e` after the cut |
|---|---|---|
| every `issue_body_topology` test | the harness sums what the HD 8 played only while a test reads it (`listen`, `levels`, the gear loop); the long simulations never do | no counter, fill or level changes — `played` is identical wherever it is read |
| `a_late_worker_costs_one_buffer_and_never_starts_a_stutter` | 128 late buffers (every window position once), not 256 | route 0: 20928 underrun frames / 274 trims (budget 8192 / 128) |
| `issue_body_topology::sibling_routes_pay_the_same_for_the_same_late_worker` | 128 late pushes, not 256 | routes 0/1: 38464 vs 12032 frames, 476 vs 172 trims — the same 26432 / 304 gap the symmetric harness recorded with 256 |
| `issue_body_topology::damage_after_two_minutes_of_any_disturbance_decays_to_zero_without_a_rebuild` (was `…_ten_minutes_…`) | 2 simulated minutes of history, not 10 | all 4 runs (insert off/on × 2 seeds): 5120–5184 underrun frames / 80–81 trims in the last 30 s |

## Hypotheses

| # | Hypothesis | Verdict | Evidence |
|---|---|---|---|
| H1 | With the insert on, the return is written on BOTH heads' routes, and those are the same physical channels (routes 0 and 2 both Main `[0,1]`): the device sums two copies — +6.02 dB in step, a comb one buffer wide when they rest a buffer apart (the measured fill 64 vs 128) | CONFIRMED, FIXED (this branch) | RED on 93ada6fc9 + option 2: `insert_on_the_return_reaches_each_physical_output_once` — `{[10, 11]: 2, [0, 1]: 2}` copies; `the_return_plays_at_the_single_head_level_on_every_tail_output` — +6.02 dB on out 0 (two heads 0.19996913, one head 0.099984564); `two_guitars_through_the_loop_never_peak_above_the_single_head_rig` — 0.4 vs 0.2; `a_unity_loop_switched_on_keeps_guitar_1_at_the_level_it_had_with_the_loop_off` — −6.02 dBFS → +0.00 dBFS at the switch. On the issue body's topology only Main doubles (`each_physical_output_plays_each_source_once_with_the_insert_on_or_off`, `two_guitars_through_the_loop_never_peak_above_the_one_e_s_rigs`: out 0 and 1 +6.02 dB, `[24,25]` and `[10,11]` right). All GREEN after `engine::insert_return_routes`. |
| H2 | The chain volume is applied on both sides of the insert cut (the send AND the tail after the return) | CONFIRMED, FIXED (this branch) | RED: `the_chain_volume_acts_once_with_the_insert_on_or_off` — 50 % moves Main −6.02 dB with the insert off, −12.04 dB with a unity loop on. GREEN once only chain outputs take the volume (`applies_chain_volume`); it was still RED after H1's fix alone. |
| H3 | The #953 drift guard learns a level from one bad window (an empty ring at the window's closing callback, a late push) and then cuts the cushion on every clean window: 1 trim + 1 buffer of silence every ~2 windows (2.7 Hz) until a rebuild — the "loop" | CONFIRMED (tests); fix in the working tree (option 2) | RED on the #980 merge, identical numbers: `an_empty_ring_at_the_closing_callback_is_not_a_level` (19, 1216); `a_late_worker_costs_one_buffer_and_never_starts_a_stutter` 60096 frames / 758 trims (256 late buffers; 20928 / 274 with #991's 128); `damage_after_ten_minutes_of_any_disturbance_decays_to_zero_without_a_rebuild` 5120 / 80 in the last 30 s (the symmetric `decay_and_device_stall` version, never committed; the committed `issue_body_topology` one is now `damage_after_two_minutes_…`, #991); `late_worker_recovery` ×2 192 / 128 frames; `once_the_worker_is_back_on_time_every_route_is_clean_again` (640, 10). All GREEN on the working tree (baseline run 2026-09-28, before the H1/H2 change), and GREEN again 2026-09-28 20:1x on the tree that also carries H1/H2/H20 and the start-order landing (option 2 ported onto 93ada6fc9; the stash `979-option2-prod-wip`, `fa4afffda`, dropped), together with the `issue_body_topology` duplicates, #980's `issue_980_a_primed_route_is_never_trimmed_below_its_prime` and `engine/tests/issue_979_deep_cushion_keeps_its_level.rs` (a no-regression pin: already green on the #980 base). |
| H4 | #965 left a convolver-fed route resting on the bare hand-off (rest 64 = the buffer it pops, nothing queued): one period late = a gap | CONFIRMED; fix = option 2, one device buffer of slack (+64 frames, +1.45 ms at 64), owner-approved | `infra-cpal` `a_worker_one_period_late_for_a_moment_costs_nothing_and_nothing_after`: 64 underrun frames per late period before, GREEN on the working tree; with it `a_missed_output_cycle_is_shed_with_a_crossfade_never_dropped` (16 failures before) and the 6 `issue_979_slack_rest` tests (e.g. 48 failures over 32/64/128-frame buffers before), all GREEN 2026-09-28 20:1x. Round-3 differential: GREEN on v0.5.0 (512-frame IR prime), RED on v0.5.1. |
| H5 | A second buffer on the insert loop's send | NOT AUTHORIZED | `with_the_insert_on_a_worker_one_period_late_costs_no_route_any_audio` stays RED and uncommitted (owner decision). |
| H6 | The #670 worker's push landing before some output callbacks and after others (phase swing) makes the guard trim (`babac2a96`) | REFUTED as the rig's mechanism | #965 measured every output callback 0–5 µs after the input on this interface; the 6 `push_phase_swing` REDs were refuted by review (within-cycle landing is not the HAL's). `babac2a96` did not fix the rig and was reverted. |
| H7 | The guard's level should follow the lowest clean floor upward (`00f90efa2`) | REFUTED as a fix | Traded up to +5.8 ms per route without the owner's approval and the rig kept underrunning; reverted (`a94709d1b`, `2a0e49b0c`). |
| H8 | #967 (insert bridge, fast chain switch) introduced the bug | REFUTED | 68 tests ported to v0.5.0 and v0.5.1 give the same 25 RED / 42 GREEN; the v0.5.1 difference is #965's cushion sizes (H4), and the H1 doubling is RED on both. |
| H9 | Each head's block graph is built twice and both copies play | REFUTED | The 4 NAM loads are the #85 per-output pipelines (2 heads × 2 outputs); `insert_off_every_route_has_exactly_one_writing_pipeline` GREEN. |
| H10 | A replaced runtime or a stale stream keeps writing a live route after a switch | REFUTED | `stream_layer_replaced_runtime` 5 GREEN; its one RED (`a_regrouping_loop_switch_never_lands_on_the_streams_of_the_other_grouping`) needs a door order the app never takes. `stored_structure` 6 GREEN. |
| H11 | An output callback's early-return path leaves a stale buffer (frozen counters with audio looping) | REFUTED | `output_write_through` 7 GREEN. |
| H12 | Device notifications rebuild the chain on their own (the 3 rebuilds in 40 min) | REFUTED for device events | `device_notifications` 6 GREEN; the send route came and went with those rebuilds, so the insert state really changed — who dispatched it is OPEN (H17). |
| H13 | The worker's 100 µs sleep-poll misses deadlines | REFUTED | ~21 % busy while broken; the late buffers were memory-pressure stalls (#980 H14, fixed there). |
| H14 | A lost `processing.try_lock` drops input buffers | REFUTED as the cause | `input_busy_skips` 0 in every live reading (#980 H2). |
| H15 | Two heads are summed into one route in our code (the send) | NOT THE STACKING | The send carries both guitars by the #967 design (one mono send to the pedals); `the_send_carries_each_guitar_once_every_frame` GREEN. `no_route_carries_both_guitars` stays RED on the send only. |
| H16 | Insert OFF (the 22:31 capture): a head's two routes, or two heads' routes, rest a buffer apart after their streams start in a different order — the same guitar twice, 1.45 ms apart | OPEN | `sibling_route_divergence` ×2, `a_route_whose_output_started_late_rests_with_its_siblings` still RED. |
| H17 | Something dispatches scene/insert changes with nobody touching the rig (3 rebuilds in 40 min) | OPEN | Not tested at the command layer. |
| H18 | The return ringing with guitar 2 muted (~−10 dBFS, ~610 ms self-similarity) is a feedback path through the HD 8 mixer or the pedals' own repeats | OPEN | Not measured since; with H1 fixed the return reaches Main once. |
| H19 | +10.1 dBFS on group 0 and `dsp_latency_ms` 8.48 → 15.62 come from the owner's NAM/IR gain and CPU contention | OPEN | `quality_probe_level` 7 GREEN with jmp1 + V30; the owner's models were not probed. |
| H20 | A click on an in-place scene edit with the insert on | CONFIRMED, FIXED (this branch) | RED on the working tree (H1/H2 + option 2): `insert_on_an_in_place_scene_edit_is_inaudible` — `route 0 [0, 1]: frame Some((192, 0.0)) after the edit left the steady level 0.20000048`. Cause, probed: the heads feeding the send have no blocks (the insert is the chain's first block), so `update_chain_runtime_state` passed them no old nodes and `build_input_processing_state` treated them as new — fade state (input, blocks, `fade_in_remaining`) before the edit `[([0],0,0), ([1],0,0), ([2,3],2,0)]`, after `[([0],0,128), ([1],0,128), ([2,3],2,0)]`. The 128-frame fade-in (first sample × 0) went out on the send, came back through the SYN-2 one cycle later and reached the tail routes at frame 192 (3 cycles) as a zero plus a 2.9 ms dip. GREEN once a kept pipeline continues the old one's fade at the swap (`continue_kept_fades`). |
| H21 | During an in-place edit the worker plays the live pipelines without their blocks (dry DI, or the dry return) | OPEN | Code reading only: Step 1 of `update_chain_runtime_state_impl` `mem::take`s every live pipeline's blocks under the lock, Step 2 builds the new states OUTSIDE it, and the worker's `try_lock` can run the emptied pipelines in between, for as long as Step 2 takes (a rebuilt NAM/IR block: ms to tens of ms). The engine harnesses call the update between cycles, so no test can see it yet; a RED needs the worker to run inside Step 2 deterministically. |
| H22 | The splice lives upstream of OpenRig's DSP: the HD 8 input stream as the HAL delivers it (to every client) jumps by a fraction of a sample every 256 frames while the rig is in the broken state, and a chain off/on in the SAME OpenRig process clears it (15:13:34 the owner toggled the chain, pid 91294 kept running 2 h 26 min; the 15:14 input reads 1.0) — so OpenRig's open streams drive the HD 8 input into the slip, and reopening them releases it | CONFIRMED by the 30/09 PM A/B (H28); the mechanism is OPEN | 29/09 row above: the same splice, same phase, in an independent process's recording of In 2 and In 3/4. Next: the rolling watch records the HD 8 inputs every ~20 s and keeps the file when any input reads ≥ 5, to catch the onset and what OpenRig did right before it (routes log, app stderr). |
| H23 | A block's internal 256/512-frame processing (VST3 `block_size=512`, partitioned IR) makes the splice | REFUTED as the source | The splice is already in the HD 8 input recorded outside OpenRig (H22), and the offline render of the live cab + CloudReverb shows none. |
| H24 | The broken state starts at OpenRig's first underrun + latency trim on the chain (not with the mixer): the 30/09 onset (09:56:20 trim on the insert routes, 09:56:54 on all) lands between two input readings 1.1 (09:56:18) and 39.4 (09:56:44), the counters then freeze while the splice stays, and a rebuild clears both — the 24/09 22:31 capture has the same shape (trims → frozen counters → still "looped") | OPEN — one onset measured | 30/09 row above. Weakens H22's "every channel": on 30/09 In 2 (a guitar not in any of OpenRig's loops) stayed clean while In 1 and the SYN-2 return In 3/4 carried the splice, so the input splice may arrive through OpenRig's own outputs (insert send → pedals → SYN-2 → In 3/4) rather than from the driver. Mixer ruled out as the origin: the same signature on 29/09 14:56 in `Mixer Bypass` with v0.5.2. Next: the watch now grabs OpenRig's output (process tap) and the inputs together as soon as an input reads ≥ 5. |

## 2026-09-30 — came back on a build that carries the #979 fix

Build: `feature/issue-328` (contains `bug/issue-979` up to `5a7d15a99`),
debug `adapter-gui` from `.solvers/issue-328`. The owner: the sound "blew up
out of nothing", "a box of bees", on every speaker, and stayed that way
without switching the chain off.

**Rig.** Chain `rig:input-4` (DIGITAL), preset AMBIENCE: Split → Mix
(A `nam_fender_68_custom_deluxe_reverb`, B `nam_diezel_hagen`), then a VST3
CloudReverb. E/S `guitarra-1` + `guitarra-1-syn5050`. Routes: group 0 →
`[0,1]` (Main) and `[24,25]`; group 1 → `[4,5]`. HD 8, 44.1 kHz / 64. Path B
had been swapped on the running chain a few minutes before.

**Measured.**

| Reading | Value |
|---|---|
| `openrig://routes`, during the symptom | group 0: underruns 3520 → 3776 (64-frame steps), dropped 1664 / 3712, `latency_trims` 9, `input_busy_skips` 50; group 1: underruns 1792 → 1856, dropped 5568, trims 2, skips 27; fill 64. Log warnings 21:54:15, 21:55:25, 21:56:53, 21:57:24Z. Then no change over 5 s while the sound was still broken. |
| Main loopback (HD 8 In 9/10), 30 s | peak −27.8 dBFS, rms −42.6, no sample ≥ 0.98. A step at the edge of 73 % of the 64-frame buffers (second difference > 5× the buffer's interior; 23 % > 20×); energy at the buffer edge 8.6× the median phase: a 689 Hz (44100/64) buzz. |
| Raw In 1, same 30 s, recorded by an independent client (Python `sounddevice`, not OpenRig) | second difference at the buffer edge: median 6.5e-4 vs 5.2e-6 inside (124×). The input the Mac hands to every client already had the steps. |
| Offline render of the same preset (`openrig-render`, DI) | clean: A+B sum exactly, peak −3.1 dBFS. |
| Host load | `qa_audit` (another session) 90 % → 662 % CPU, a `rustc` build ~87 %, `adapter-gui` 99–152 %. No CoreAudio overload in `log show`. |

**Hypotheses.** (Numbered H22–H25 on `feature/issue-328`; renumbered H25–H28
here because `hotfix/issue-979` had already used H22–H24.)

| # | Hypothesis | Verdict | Evidence |
|---|---|---|---|
| H25 | A split feeding two outputs shares state between the two per-output pipelines (#85): each output's buffer is not the successor of its previous one | REFUTED | `issue_328_split_seam_tests::a_split_feeding_two_outputs_plays_a_continuous_tone_on_each` GREEN (split with stateful paths + a stateful tail block, outputs `[0,1]` and `[24,25]`). |
| H26 | The in-place swap of path B leaves the split producing broken buffers | REFUTED (engine level) | `issue_328_split_seam_tests::swapping_path_b_on_a_running_split_keeps_every_output_continuous` GREEN. That test swaps inside the engine; on `feature/issue-328` a path-B swap is a structural #881 stream replacement (see H30), which it does not cover. |
| H27 | The input stream CoreAudio delivers is already discontinuous, and the high-gain amps turn it into the buzz | CONFIRMED (the cause is H28, not host load) | the raw In 1 capture above (124×, a step on 95 % of the buffers), recorded outside OpenRig. Control: the owner's earlier DI takes of the same guitar (`~/.openrig/evaluations/gravity-john-mayer/di/jpfaria-*.wav`) read 1.05–1.19 on the same measure. A/B on the live rig, owner playing throughout, 8 s each, independent recorder: chain ON (broken) → raw In 1 edge ratio 119.4, Main loopback 18.9; chain OFF (`toggle_chain_enabled`) → In 1 1.03; chain ON again → In 1 1.02, Main 1.05, and the owner heard it back to normal. In 1 and the Main loopback step at the same phase of the 64-frame grid. A 576-frame self-similarity in the same capture is the note's own period, not a replayed buffer (REFUTED as a loop). |
| H28 | OpenRig's own stream state breaks the HD 8's input for every client on the Mac (not the host load): while the chain is in the broken state, even a separate process reads a stepped In 1 | CONFIRMED | the A/B above: the same input, the same load, only the chain toggled; the steps vanish from the device input the moment the chain's streams close and do not come back when they reopen. It holds for In 1 specifically: a guitar DI that no OpenRig output can reach, so the steps there cannot be OpenRig's own output coming back. Which part of the chain's CoreAudio stream (input IOProc, buffer size, driver state) does it is OPEN (see the recon below). |

## 2026-09-30 — code recon: what can hold the HD 8 input broken

Read-only recon of `bug/issue-979` at `8d5893915` (release/v0.6.0) plus
`cpal` 0.17.3 / `coreaudio-rs` 0.14.0, seven candidates, the top three each
checked by an independent verifier asked to refute it. The owner ruled out a
DI, looper or metronome as the trigger: the symptom happens with none of them
running.

Facts the code gives:

- A chain toggle OFF closes every stream of **that** chain before anything
  reopens (`controller.rs` `kill_chain_streams`: drain, remove, 50 ms); the
  whole controller drops only when nothing else runs (chains, pending
  activations, DI, metronome: `controller_liveness.rs`). A structural edit
  (#881, since `27f522208`, in v0.5.1+) starts the new streams before it drops
  the old ones, so OpenRig's IO on the device never pauses.
- On `feature/issue-328` every block model swap is structural (`Split`
  `model_identity` includes its paths; `chain_structure_signature` includes
  every block's `model_identity`).
- OpenRig writes no CoreAudio property itself. `cpal` writes
  `kAudioDevicePropertyBufferFrameSize = Fixed(n)` at device scope on every
  build, and the nominal rate only when it differs. A rebuild and a toggle-ON
  write the same values.
- The macOS F32 input callback only copies into the per-slot ring; the
  dsp-workers (one per slot) are time-constraint threads declaring 85 % of a
  period at start and join the bound device's IO workgroup
  (`dsp_worker.rs`). The HAL IO thread joins the workgroup on its first
  callback, enumerating every device and allocating on the audio thread
  (`ensure_joined_input`/`_output`, guard forgotten).
- Nothing in `crates/` observes `kAudioDevicePropertyBufferFrameSize`,
  `kAudioDeviceProcessorOverload` or `kAudioDevicePropertyDeviceIsRunningSomewhere`.

| # | Hypothesis | Verdict | Evidence |
|---|---|---|---|
| H29 | One late OpenRig IO cycle under load re-anchors the driver's shared input timeline at OpenRig's 64-frame cadence; the state holds while OpenRig's IO runs without a gap and only the toggle (a real stop) clears it | REFUTED as stated | Verifier: one re-anchor gives one discontinuity, not a step on 73–95 % of the buffers for minutes; a mis-anchored ring skips or repeats whole samples, while 29/09 measured a sub-sample jump (−0.48×, R² 0.94), which needs an interpolating read every cycle. No HAL overload was ever seen at an onset (PM `log show` none; the AM listener started after the onset). The 64 grid may be the recorder's own cycle: its block size was not recorded. |
| H30 | The #881 make-before-break of a structural edit (the path-B swap minutes before the PM symptom) latches the state | REFUTED as the trigger | 30/09 AM onset came 4 min after the last start with no rebuild in between, together with the first underrun + trim; 24/09 was 8 min clean after a rebuild; 3 rebuilds in the 40 min watch and no symptom; the owner was playing through the PM swap and heard nothing then. A toggle-ON writes the same device properties as a rebuild. |
| H31 | A second opener in the process (DI/looper at `Fixed(256)`, metronome at 512) re-sizes OpenRig's IO cycle on the HD 8 | REFUTED for 30/09 | The owner: it happens with none of them running. The 30/09 seams sit at 52 mod 64 (frames 7156, 7796, 18932), not on a 256 or 512 grid. A DI or metronome survives a chain OFF, yet the chain-OFF take read clean. The real inconsistency (64 / 256 / 512) stays a separate fact; it does not fail the way the rig failed. |
| H32 | The RT dsp-workers (85 % each, joined to the HD 8 workgroup) plus host load starve OpenRig's HAL IO thread and give the late cycle | OPEN — no cross-client path | Explains the load counters (underruns, trims, skips at 662 % host CPU). On its own it cannot reach another process's In 1; it needed H29's latch, which is refuted as stated. The workgroup join was measured WORSE in #670 and re-added in #760 on reasoning alone (the #760 reopen was #251). |
| H33 | OpenRig's own stepped output re-enters the HD 8 inputs (insert send → pedals → SYN-2 → In 3/4; Main loopback In 9/10) | REFUTED as the full cause | Explains In 3/4 and In 9/10 only. In 1 (PM 124×, AM 39.4) and In 2 (29/09, 239×) are guitar DIs with no path from any OpenRig output. |
| H34 | The HAL IO thread's workgroup join on its first callback (device enumeration, CFString, allocation) lengthens one cycle after each (re)open | OPEN — out of scope here | Once per thread, so no persistence and not the minutes-later onset; breaks invariant 8 regardless. |
| H35 | Processing-lock contention and worker saturation drop input buffers | REFUTED as the device-level cause | New fact vs H14: `input_busy_skips` moved 50 / 27 on 30/09 PM. A skip inside OpenRig cannot change what another process reads from In 1. |

Discriminators not yet run (the next capture must carry them): a recorder
whose block size is not a multiple of 64 (e.g. 100), logging per-cycle
`mSampleTime`/`mHostTime`; `DeviceIsRunningSomewhere` read through the
owner's toggle; a known clean tone from a non-OpenRig source on an idle
input; OpenRig's outputs silenced with its input stream kept open; a #881
rebuild (not a toggle) during the broken state; OpenRig's buffer size on
29/09 (the 256 fold). Also unknown: whether In 2 carried signal on 30/09 AM
(its clean 1.1–1.7 only counts if it did), and whether any SRC or aggregate
device sits in the HD 8 path (the fractional jump).

**Fixes (owner-approved 2026-09-30).**

1. Take the dsp-workers out of the HD 8 IO workgroup, keeping their RT
   promotion and budget (no latency change). **On hold:** it rests on
   H29 + H32, and H29 is refuted as stated. It ships only if a hardware red
   (the HD 8 input stepping for an independent recorder) goes green with it.
2. Watch the input OpenRig receives; when it arrives stepped at the buffer
   edge for about 1 s with signal present, restart that chain's streams
   automatically: the same thing the owner's toggle does, which H28 measured
   as the cure. Automatic recovery, like `reconnect_audio`, not a `Command`.

**Owner verdict on fix 2 (2026-10-01): not a fix.** A restart cuts the sound;
on stage that is a dropout in the middle of a song, so automating the toggle
only moves the failure. The deliverable of #979 is still the root cause: why
the HD 8 input arrives stepped while a chain runs. The automatic restart stays
as recovery, but every restart must leave a mark: the evidence of that
moment, kept so the cause can be found. Investigation continues.

**#979 stays open until the root cause is found and fixed** (owner,
2026-10-01). No commit or PR uses a closing keyword for it (`closes`,
`fixes`, `resolves #979`); a mitigation PR references it as `Refs #979`.

**Fix 2 as built.**

- Measure: per input channel, the mean |3rd difference| at each phase of the
  buffer, folded at the buffer length; ratio = max / median phase. The
  recorded broken In 1 (`engine/tests/fixtures/issue_979/in1_*.f32`) reads
  107–186; the clean takes read 1.1–1.3.
- Detector (`engine/src/input_seam_detector.rs`): 0.25 s windows, a window
  counts only with signal above 1e-9, stepped when the ratio is above 5; four
  stepped windows in a row (1 s) trip it, four clean windows in a row clear
  it (it does not latch: see the app test below). A window without signal
  holds both counts. A buffer lost to a busy lock is a `discontinuity()`, not
  a seam.
- Engine (`runtime_input_seams.rs`): one detector per input channel of each
  input runtime; every callback stores into `ChainRuntimeState::input_stepped`
  whether any of them is tripped now. No allocation on the audio thread.
- Controller (`infra-cpal/src/controller_stepped_restart.rs`):
  `stepped_input_chains` lists the chains whose runtime tripped;
  `restart_chain_streams` does the toggle — `kill_chain_streams`, then the
  same activation as switching the chain on. A chain that is off is left alone.
- GUI (`adapter-gui/src/stepped_input_tick.rs`): the 2 s health tick restarts
  each stepped chain and then leaves it alone for 30 s (15 ticks), per chain,
  so a restart that does not cure it never cuts the sound every 2 s and one
  chain's wait never delays another. The meters follow the new runtime by its
  identity (#957).
- Pinned by: `input_seam_detector` tests, `issue_979_input_seam_runtime_tests`,
  infra-cpal `issue_979_stepped_restart_tests` (×5), adapter-gui
  `stepped_input_tick` tests (×6).

**Fix 2 in the app (2026-10-01, BlackHole 2ch, no HD 8).** Debug build of
`bug/issue-979`, isolated config and project (recipe:
`.claude/skills/openrig-tooling/SKILL.md` → "Opening the app yourself"), two
chains on one BlackHole device at 48 kHz / 64 frames: `rig:in-a` reads ch 0,
`rig:in-b` reads ch 1, preset volume 0 (BlackHole loops output to input). A
Python feeder plays a 220 Hz sine on ch 0 (plus a 64-frame step pattern of
0.05 when switched to "stepped") and a clean 330 Hz sine on ch 1.

- Clean input: no restart; meters −20 dBFS on both chains.
- Stepped on ch 0: `rig:in-a` restarted ~2 s after the switch
  (`controller_stepped_restart: chain 'rig:in-a': input arrives stepped,
  restarting its streams`), then every 30 s while it stayed stepped. The
  meters kept reading after each restart (−16.5 dBFS). `rig:in-b` was never
  restarted; its callbacks kept counting with 0 underruns.
- **Defect found:** with the trip latched, a restart came 24 s after the input
  was clean again — the flag still held the stepped verdict from before the
  previous restart's wait. Fixed red-first: the trip clears after 1 s of clean
  input (`a_trip_clears_after_one_second_of_clean_input`,
  `the_mark_clears_once_the_input_is_clean_again`).
- Run 2, with the fix (03:45–03:47 local): 20 s clean → 0 restarts; stepped
  at :48 → restart at :50; clean from :51 for 45 s → no restart (the 30 s
  boundary passed silent); stepped again at 46:36 → restarts at 46:38 and
  47:08. `rig:in-b`: 0 restarts, 0 underruns, 97 812 callbacks. Device
  rates and the project file unchanged afterwards.
- On the first enable, in both runs, the chain enabled first (`rig:in-a`)
  logged 64 underruns and one "audio overload" warning, at startup only
  (debug build); 0 underruns after its restarts.
- OPEN: during this test (03:33–03:37 local) the owner reported trouble on the
  HD 8 in another OpenRig. Not measured. A second OpenRig process opens
  CoreAudio and enumerates every device even when its chains use only
  BlackHole; whether that disturbs an HD 8 session is unknown. Rule since then
  (CLAUDE.md): an agent never opens OpenRig while another one is running.


## Shipped

Nothing merged yet. On `bug/issue-979` (commit at the end of the #979 work):

| Change | What | Pinned by |
|---|---|---|
| `engine/src/insert_return_routes.rs`, `runtime_segments.rs` | While an insert cuts the chain, the return writes the first tail route on each physical output (same device, same channels); the other E/S's route there is written by nothing, so #947 does not build it. No latency change: the routes the return keeps are the ones it had. | H1 tests above |
| `runtime_graph.rs` (`switch_owned_routes`, `heads_make_one_runtime`) | A runtime owns routes it does not write (#967) only when the chain is one runtime in every insert state (one input entry). The two-E/S loop runtime used to own all 5 routes; with the return on 3 of them, the streams of routes 2 and 3 held it with nothing to pop (the #947 stream-layer pins went RED on that). Those streams now hold no runtime and play silence. | infra-cpal `issue_979_stored_structure_tests` ×5, `output_write_through::the_route_counters_the_owner_reads_follow_every_callback_through_live_switches`, `device_notifications::after_a_device_notification_a_scene_switch_keeps_the_streams` |
| `runtime_state.rs` (`applies_chain_volume`), `runtime_graph_assemble.rs`, `runtime_graph_update.rs`, `runtime_output_process.rs` | The chain volume scales chain outputs, never an insert send. At 100 % nothing changes; below or above 100 % the pedals now get the loop's input at the chain's own level and the tail takes the knob once. | `the_chain_volume_acts_once_with_the_insert_on_or_off` |
| `runtime_graph_update.rs` (`continue_kept_fades`) | An in-place edit keeps the fade position of every pipeline it keeps (same input, same routes, no block built fresh), read at the swap. A blockless head feeding the insert send no longer fades in from silence on every scene edit. No latency change. | `insert_on_an_in_place_scene_edit_is_inaudible` |
| option 2 (`elastic_drift_guard.rs`, `elastic_hand_off.rs`, `route_cushion.rs`, `elastic_buffer.rs`) | The guard never learns a level below the route's rest; a convolver-fed route keeps one buffer of slack (+64 frames). See `docs/audio-config.md` → "How a route's cushion is sized". | H3 / H4 tests above |

Hunt tests (uncommitted until the ship) whose expectations were the doubled
loop-on topology the rig measured — 5 routes, the return on routes 2 and 3 —
now expect the structure H1 fixes (3 routes with the loop on: `[0,1]`,
`[10,11]`, `[3]`; routes 2 and 3 written by nothing): 
`insert_on_heads_feed_the_send_and_the_return_feeds_the_tail` (return
`[0,1,2,3]` → `[0,1]`), `k_scene_switches_keep_five_routes_…` renamed
`k_scene_switches_keep_three_routes_with_the_insert_on_and_four_with_it_off`
(5 → 3), `a_healthy_two_head_rig_is_never_cut` (5 → 3),
`frame_integrity` (`written_by`: the loop-on scene writes `[0, 1, 4]`, and a
route a scene does not write must play nothing), `head_route_offset` (with
the loop on only routes 0/1 form a Main + Out 2 pair), and infra-cpal
`stream_layer_replaced_runtime` (3 output streams carry a runtime with the
loop on, not 5; the streams of routes 2 and 3 must stay silent).

## Open

- The owner's ear on the rig: insert on (no stacked sound on Main, same level
  as insert off), insert off, 15+ minutes.
- H16–H19, H21.
- `docs/audio-config.md` → **Slack** still describes the port as it was
  before the start-order change now in the tree: the *At the start* bullet
  lands a route "inside its band" and drops only what queued "above the
  band", and the closing sentence says a route no convolver feeds rests at
  the hand-off and, after a staggered start, at its full ring. The tree
  lands on `need` (the band's floor) and lands a lean route on its
  producer's clock once, at its producer's first hand-off
  (`elastic_hand_off::at_first_hand_off`). To be rewritten with the
  start-order fix.
- Two E/S outputs that overlap only partly, or list the same channels in
  another order, still both get the return (the rule is "same device, same
  channel list").
- 2026-09-28, full `engine --lib` (857 pass / 19 fail) and `infra-cpal --lib`
  (277 / 2) on the working tree: the 7 H1/H2 tests are GREEN. Two COMMITTED
  engine tests are RED, both on chains with no insert, so outside H1/H2 and
  inside the option-2 / start-order work (`elastic_hand_off`,
  `route_cushion`): `issue_965_insert_latency::no_fresh_route_is_cut_and_a_convolver_fed_one_is_born_at_rest`
  ("cab: route 0 (target 128) rests at 64 frames between periods", left 64,
  right 128) and
  `runtime_output_route_stats_tests::each_route_reports_the_frames_queued_in_its_own_cushion`
  ("route 1 popped one period", left 0, right 64). The first stops at its
  `cab` case, so its two insert cases did not run. Both must be settled
  before the ship. The infra-cpal reds are the unauthorized H5 pin and the
  H10 door-order test.

## 2026-09-30 — came back on a build that carries the #979 fix

Build: `feature/issue-328` (contains `bug/issue-979` up to `5a7d15a99`),
debug `adapter-gui` from `.solvers/issue-328`. The owner: the sound "blew up
out of nothing", "a box of bees", on every speaker, and stayed that way
without switching the chain off.

**Rig.** Chain `rig:input-4` (DIGITAL), preset AMBIENCE: Split → Mix
(A `nam_fender_68_custom_deluxe_reverb`, B `nam_diezel_hagen`), then a VST3
CloudReverb. E/S `guitarra-1` + `guitarra-1-syn5050`. Routes: group 0 →
`[0,1]` (Main) and `[24,25]`; group 1 → `[4,5]`. HD 8, 44.1 kHz / 64. Path B
had been swapped on the running chain a few minutes before.

**Measured.**

| Reading | Value |
|---|---|
| `openrig://routes`, during the symptom | group 0: underruns 3520 → 3776 (64-frame steps), dropped 1664 / 3712, `latency_trims` 9, `input_busy_skips` 50; group 1: underruns 1792 → 1856, dropped 5568, trims 2, skips 27; fill 64. Log warnings 21:54:15, 21:55:25, 21:56:53, 21:57:24Z. Then no change over 5 s while the sound was still broken. |
| Main loopback (HD 8 In 9/10), 30 s | peak −27.8 dBFS, rms −42.6, no sample ≥ 0.98. A step at the edge of 73 % of the 64-frame buffers (second difference > 5× the buffer's interior; 23 % > 20×); energy at the buffer edge 8.6× the median phase: a 689 Hz (44100/64) buzz. |
| Raw In 1, same 30 s, recorded by an independent client (Python `sounddevice`, not OpenRig) | second difference at the buffer edge: median 6.5e-4 vs 5.2e-6 inside (124×). The input the Mac hands to every client already had the steps. |
| Offline render of the same preset (`openrig-render`, DI) | clean: A+B sum exactly, peak −3.1 dBFS. |
| Host load | `qa_audit` (another session) 90 % → 662 % CPU, a `rustc` build ~87 %, `adapter-gui` 99–152 %. No CoreAudio overload in `log show`. |

**Hypotheses.**

| # | Hypothesis | Verdict | Evidence |
|---|---|---|---|
| H22 | A split feeding two outputs shares state between the two per-output pipelines (#85): each output's buffer is not the successor of its previous one | REFUTED | `issue_328_split_seam_tests::a_split_feeding_two_outputs_plays_a_continuous_tone_on_each` GREEN (split with stateful paths + a stateful tail block, outputs `[0,1]` and `[24,25]`). |
| H23 | The in-place swap of path B leaves the split producing broken buffers | REFUTED | `issue_328_split_seam_tests::swapping_path_b_on_a_running_split_keeps_every_output_continuous` GREEN. |
| H24 | The input stream CoreAudio delivers is already discontinuous, and the high-gain amps turn it into the buzz | CONFIRMED (the cause is H25, not host load) | the raw In 1 capture above (124×, a step on 95 % of the buffers), recorded outside OpenRig. Control: the owner's earlier DI takes of the same guitar (`~/.openrig/evaluations/gravity-john-mayer/di/jpfaria-*.wav`) read 1.05–1.19 on the same measure. A/B on the live rig, owner playing throughout, 8 s each, independent recorder: chain ON (broken) → raw In 1 edge ratio 119.4, Main loopback 18.9; chain OFF (`toggle_chain_enabled`) → In 1 1.03; chain ON again → In 1 1.02, Main 1.05, and the owner heard it back to normal. |
| H25 | OpenRig's own stream state breaks the HD 8's input for every client on the Mac (not the host load): while the chain is in the broken state, even a separate process reads a stepped In 1 | CONFIRMED | the A/B above: the same input, the same load, only the chain toggled; the steps vanish from the device input the moment the chain's streams close and do not come back when they reopen. Which part of the chain's CoreAudio stream (input IOProc, buffer size, driver state) does it is OPEN. |

## Related

- #980 — the late dsp-worker (memory pressure) that triggered H3/H4 on the rig.
- #947 (routes built only for what a runtime writes), #953 / #965 / #969
  (route cushion and drift guard), #967 (insert streams), #440 (chain volume),
  #85 (a stream is one input × one output).
