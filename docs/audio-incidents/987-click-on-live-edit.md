# #987 — a small click whenever a running stream is modified

Status: **FIX ON THE BRANCH, awaiting the owner's ear** — every mechanism
reproduced by a test; all eight tests green 3 of 3 runs; `cargo test
--workspace` green.
Issue: https://github.com/jpfaria/OpenRig/issues/987 · Branch: `bug/issue-987`

## Symptom (reported)

- 2026-09-28: "quando eu ligo e desligo um bloco, troco de cena… quando o
  stream é modificado, dá um pequeno estalo". The owner asked for the fix
  without touching what already sounds good.

## Symptom (measured)

Measured offline (no rig mutation) by
`crates/infra-cpal/src/issue_987_live_edit_click_tests.rs`: a steady 233 Hz
tone at 0.5 peak through the chain's live slot, every output sample scanned
for a step (second difference above 10x the tone's own curvature) or a gap
(silent run longer than 2 frames).

| Path | Edit | Reading (2026-09-28, debug build) |
|---|---|---|
| Fresh off-thread rebuild (chain without a VST3) | scene switch | 16 step frames, worst 1016x, 6-frame silent run, at the first switch |
| Fresh off-thread rebuild | block that started off, turned on | 2 step frames, worst 1076x |
| Fast block toggle (block with a live processor) | off/on x4 | clean |
| In place (chain holding a VST3), lockstep audio | scene switch, block that started off turned on | clean |
| In place, audio on its own thread, real IR cab (repo fixture) | cab switched by scene | 30-33 step frames, worst 1065-1131x, 3 of 3 runs |
| In place, audio on its own thread, real IR cab | cab that started off, turned on | 4-10 step frames, worst 563-957x, 3 of 3 runs |

Timeline probe of the in-place cab switch (per-callback peak, audio thread at
a 250 µs pace; the edit took 0.7-2.4 ms): every switch plays 2-3 callbacks at
peak 0.43-0.50 — the raw input, no block at all — then drops back to the
processed ~0.09. `input_busy_skips` rose by 1 over 3 switches (one whole
callback lost to the processing lock).

## Rig

The owner's chains hold two VST3 reverbs plus NAM and IR (see #980), so every
live edit on them takes the in-place path (#779). Quantum HD 8, 44.1 kHz,
64 frames (1.451 ms period).

## Reproduce

```
cargo test -p infra-cpal --lib issue_987
```

The in-place cases need the git-lfs IR fixture
(`crates/engine/tests/fixtures/plugins/ir/marshall_4x12_v30`); without it they
print BLOCKED and return.

## Hypotheses

| # | Hypothesis | Status | Evidence |
|---|---|---|---|
| H1 | The fresh off-thread rebuild publishes the new runtime into the live slot with no crossfade: the old output stops, the new runtime starts from its 128-frame fade-in from silence and cold DSP | CONFIRMED | `switching_scene_while_the_tone_plays_does_not_click` red (6-frame gap = the fade-in from 0); `controller_rebuild_queue.rs` `poll_pending_rebuilds` → `slot.publish` |
| H2 | The in-place update empties the live pipelines (`std::mem::take` of every block, Step 1 of `update_chain_runtime_state_impl`) and builds the new nodes outside the lock, so the audio plays the raw input for the whole build and snaps back with no fade | CONFIRMED | probe: 2-3 callbacks at the input's own peak (0.50) per switch; red only with the audio on its own thread |
| H3 | The in-place update's two lock sections collide with the audio thread's `try_lock`, losing a whole callback | CONFIRMED, rare | `input_busy_skips` +1 over 3 switches at a 250 µs callback pace (5.8x the device rate) |
| H4 | The first-edit silent buffer seen in the in-place probe is a click in the app | REFUTED | harness artefact: the seeded runtime used the default elastic target, the edit the live one (fill 256 → 64), so the route was rebuilt primed; the app activates and edits with the same `elastic::elastic_targets`. The tests now settle the chain first. |
| H5 | The per-block 128-frame dry/wet fade of the fast toggle is too short and clicks | REFUTED for the level blocks tested | `turning_a_block_off_and_on_while_the_tone_plays_does_not_click` green |
| H6 | A fresh node replacing a live one fades in from its DRY input with cold state: the old wet output is cut to dry on the first sample, and an IR plays one partition (64) of zeros then its onset | CONFIRMED | with H2 fixed, `switching_the_cab_by_scene_on_a_chain_holding_a_vst3_does_not_click` still red, 52-71 step frames, worst 1344-1433x; green once the fresh node warms up unheard and crossfades from the old node |
| H7 | A block turned back on by the fast toggle resumes a processor frozen since it was switched off, ramping its stale state in over 128 frames | CONFIRMED | `turning_on_a_cab_that_was_off_on_a_chain_holding_a_vst3_does_not_click`: after the first ON, the later ONs (fast path) still stepped 24-44x, first at 1.1-1.3 s; green once the re-enable warms up 512 frames first |

## Shipped

Branch `bug/issue-987` (see `docs/audio-config.md` → "A live edit never clicks"):

- H2: `engine/src/runtime_graph_prebuild.rs` builds every fresh node before the
  live pipeline is touched; `runtime_graph_update.rs` then swaps in ONE
  processing-lock section (moves only, no logging). Fresh VST3 or `Select`:
  the quiesced path as before.
- H6: `engine/src/runtime_node_handover.rs` — the fresh node runs unheard 512
  frames while the replaced node (or the dry input) plays, then a 128-frame
  crossfade; the replaced node is parked and dropped by the next edit on the
  control thread.
- H7: a re-enabled block's fade is `WARMED_FADE_IN_FRAMES` (512 unheard + 128).
- H1: `infra-cpal/src/slot_handover.rs` — the old runtime keeps playing in its
  slot while the new one warms up 1536 frames, then a 256-frame crossfade per
  output; reaped on the control side at `strong_count == 1`.
- H3: `engine/src/runtime_processing_lock.rs` — the DSP worker retries the
  processing lock up to one period (20 µs sleeps) instead of dropping the
  buffer.

Latency unchanged; the new sound starts 11-40 ms after the edit.

## Open

- An edit that needs a fresh VST3 (a VST3 that was off at load turned on, a
  VST3 model change, a bus change in front of one) keeps the quiesced path and
  its click (#779).
- Tails of a replaced delay/reverb are cut over the 128-frame crossfade; a
  replaced modulation block restarts its LFO.
- Structural edits that regroup the runtimes (insert flips, #967) rebuild the
  streams and are not covered here.
- CPU: during a handover the replaced block (in place) or the whole old runtime
  (fresh rebuild) runs in parallel for the warm-up and fade.

## Related

- #190 (per-block fade state machine), #454 (scene crossfade + spillover tail,
  whose `update_chain_runtime_state_spillover` has no production caller),
  #979 H20 (`continue_kept_fades`: a kept pipeline no longer fades in from
  silence on an in-place edit).
