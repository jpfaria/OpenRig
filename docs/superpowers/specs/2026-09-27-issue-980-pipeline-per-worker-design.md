# #980 — one runtime (one dsp-worker) per (input × output) pipeline — design

Status: approved by the owner (decisions P3 / JACK / TESTS below). Branch
`bug/issue-980`, solver `.solvers/issue-980`, base commit `40c405013`.
Implementation plan: `docs/superpowers/plans/2026-09-27-issue-980-pipeline-per-worker.md`.

## 1. Problem (measured)

On the owner's rig (chain ANAL+DIG: NAM + IR + 2 VST3, two guitars, each E/S
with outputs `[0,1]` and `[10,11]` on the Quantum HD 8) every output route
lost exactly the frames it later dropped: `openrig://routes` showed
`underruns == dropped_frames` on every route and `input_busy_skips == 0`.
Nothing held the processing lock; the producer delivered late.

Mechanism (read from the code, pinned by the red contract test
`crates/engine/tests/issue_980_one_worker_per_pipeline.rs`):

- The engine builds one segment per (output route × input)
  (`crates/engine/src/runtime_segments.rs:239-286`); every segment outside an
  insert cut writes exactly one route (`output_route_indices: vec![out_entry_idx]`).
- `group_segments_by_input` (`crates/engine/src/runtime_graph.rs:139-169`)
  groups segments by RAW input entry only, so `(Main, guitar)` and
  `(Out 2, guitar)` share ONE `ChainRuntimeState`.
- infra-cpal makes one `LiveRuntimeSlot` per runtime and, on macOS F32, one
  `dsp_worker` per slot (`crates/infra-cpal/src/stream_builder_input.rs:85-99`).
  One runtime ⇒ one worker ⇒ the two outputs of one guitar run back to back on
  one realtime thread; the second waits for the first on every buffer.
- Linux/JACK is worse: the engine groups per DEVICE there
  (`runtime_graph.rs:147-148`), the JACK-direct client binds only the first
  runtime (`crates/infra-cpal/src/stream_builder.rs:158-167`), runs one worker
  (`crates/infra-cpal/src/jack_direct.rs:199-335`) and pops route 0 only
  (`crates/infra-cpal/src/jack_handlers.rs:256`): the second output of an E/S
  is silent on Linux/JACK today.

The owner's LAW (CLAUDE.md): N streams = N isolated pipelines, isolation
includes CPU time, and "1 input and 2 outputs = 2 streams". The #716 spec
already approved "each (input, output) pair is its own isolated runtime"
(`docs/superpowers/specs/2026-06-17-issue-716-io-binding-registry-design.md:112-113,145`);
the per-entry grouping is the deviation.

## 2. Owner decisions (verbatim)

- **(P3) FIXED SLOT SET:** a chain with a bound Insert and several outputs
  keeps the SAME set of runtime slots whether the insert is on (cut) or off,
  so switching the insert never reopens audio streams (#967 guarantee); with
  the insert on, one runtime does the cut work and the others idle; with it
  off each pipeline has its own runtime/worker.
- **(JACK)** the Linux JACK path ALSO moves to one worker per pipeline.
- **(TESTS)** old tests whose assertions encode the old one-runtime-per-input
  topology may be adjusted; no sound/latency/volume check may be relaxed.

## 3. Chosen mechanisms

### 3.1 Pipeline key and dense slots (engine)

- New `crates/engine/src/pipeline_grouping.rs`. A pipeline is
  `PipelineKey { route, entry_group }`: the output route a segment writes and
  the RAW input entry it starts from. Split-mono siblings share
  `entry_group`, so they stay in one runtime per output pipeline and still sum
  before the limiter (volume invariants g02/g03). A per-segment key is
  rejected (it would double-limit siblings).
- Runtimes are keyed `(ChainId, slot)` with `slot` a dense ordinal in first-seen
  segment order. The `(ChainId, usize)` type used by ~15 infra sites is kept,
  and walking runtimes by slot walks streams in the order
  `chain_stream_io_labels` names the meter rows (fixes the 2 × 2 row order).
- A chain an enabled, bound insert cuts stays one runtime (the cut spans cpal
  indices) unless §3.5 applies.

### 3.2 Pipeline identity

- Every runtime is stamped with the pipeline it runs:
  `ChainRuntimeState::owned_pipeline: Option<OwnedPipeline { key, cpal_input_index }>`
  (`pipeline_key()` accessor). `input_cpal_index()` keeps its meaning.
- An in-place refill finds the runtime's segments by its stamp, never by its
  slot (slots are positions; the pipeline at a position moves when a guitar or
  an output joins). A missing pipeline is an error, never a wrong refill (the
  #703 double-volume class).
- `RuntimeGraph::runs_pipelines_of(chain, registry)` compares the live
  `(slot, stamp)` list against `pipeline_slots(chain, registry)`; the in-place
  path requires equality. A whole-chain runtime (no stamp: probe, tests)
  matches any pipeline at its slot.
- `input_group_ids` = the slots of `pipeline_slots` (unchanged public API);
  `chain_structure_signature` carries `pipeline_slots` itself (§3.3).

### 3.3 infra-cpal hardening (reachable once a guitar owns ≥ 2 runtimes)

- Stream teardown drains every runtime of the chain, not the first
  (`teardown_active_chain_for_rebuild`).
- Installing a chain's slots retires, on the control worker (#934), the
  slots its old topology left AND every slot replaced under the same key (it
  can hold the last reference to a superseded runtime) — `install_chain_slots`.
- A finished off-thread rebuild lands whole or not at all: every slot must run
  the same pipeline as its live slot (an unstamped live runtime accepts any);
  then every slot is published back to back (lock-free stores, nothing in
  between) and only then the graph and the drops follow. A build that does
  not fit is dropped on the worker, never half-applied.
- The off-thread live rebuild declines (synchronous path) when the live
  runtimes no longer run the chain's pipelines.
- The synchronous path rebuilds the chain's streams (a new JACK client) when
  the live runtimes no longer run the chain's pipelines, even with every stream
  signature equal (two E/S re-paired over the same I/O keep the slot ids, the
  flat I/O lists and the signatures), decided before the graph upsert so the
  teardown drains what the streams play; `chain_structure_signature` carries
  `pipeline_slots` (slot + pipeline) instead of the slot ids.
- DI loop (test/HW-harness path `set_chain_di_loop`): armed on the first
  runtime writing each output ROUTE, never per rate (LAW).

### 3.4 Linux/JACK (owner decision JACK)

- One JACK client per chain (no new libjack clients → no new #294/#308 shm
  exposure). Inside it: the process callback copies ports only; `JackPeriod`
  hands the period to every `JackPipeline` (own SPSC ring + own wake + own
  dsp-worker per runtime) and renders every chain output route from the
  pipelines that own it (`owns_output`, the cpal rule) into the device buffer.
  Two routes on the same device channels (two E/S on Main) are summed there —
  the client is the device's backend mix, as `process_output_f32_mixed` is for
  a cpal output stream — and those channels pass the same saturation guard
  (`output_limiter`); a channel one route writes is played as rendered. The
  ring slot records its period length (no zero-fill of `MAX_JACK_FRAMES ×
  ports` per pipeline per period). The hand-off is a bounded copy, a
  `try_lock` of the wake flag and one `notify_one` per pipeline per period
  (R4).
- The client joins ONE JACK server (the device of the chain's first input,
  cpal index 0) and every pipeline's worker dispatches the period at index 0,
  as the single worker did; a pipeline of another device is not fed by it.
- JACK chain removal goes through `kill_chain_streams`, like cpal: every
  pipeline drained, every slot retired, everything dropped on the worker.
- Then the engine's JACK cfg key and the engine `jack` feature are deleted:
  every platform groups the same way; the cfg-gated isolation tests run on
  Linux too.
- A kept JACK client always plays the graph's runtimes: the graph replaces a
  chain's runtimes only in a full rebuild, which now always comes with a new
  client (§3.3), and the in-place path keeps the `Arc`s. No republish into a
  live JACK client (so the JACK RT thread never ends up the last owner of a
  superseded runtime); a test pins it.

### 3.5 Fixed slot set across an insert switch (owner decision P3)

- `crates/engine/src/insert_fixed_slots.rs`: a chain that owns insert streams
  (`insert_cut::insert_owns_streams`) AND whose loop-off pipelines all read ONE
  input stream keeps `N = loop-off pipeline count` slots in both states.
  Loop off: slot k runs loop-off pipeline k. Loop on: slot 0 runs the whole
  cut, slots 1..N idle (zero segments; an idle runtime returns before any lock:
  `fed_inputs` = 0).
- Every slot has ONE identity in both states: stamp = loop-off pipeline k,
  cpal index = the one input stream, clock = that input device's rate (#736),
  owned routes = slot 0 every route, slot k its own pipeline's route
  (`switch_owned_routes` field). So each output stream holds the same slots
  whichever state built it, `input_group_ids` is `[0..N]` in both states, and
  the switch stays a DSP rebuild on the open streams (#967).
- `process_output_f32_mixed` picks its path by the number of held runtimes
  that WRITE the output (0 → silence, 1 → the byte-identical single path,
  ≥ 2 → the existing backend mix). Without it a held idle slot would send the
  writer through the backend limiter a second time (volume #10).
- In-place switches (VST3 chains #779, JACK, the synchronous upsert) rebuild
  every runtime of the chain together: (1) plan all (fail before touching
  any); (2) while every runtime still plays with its own processors, walk each
  new segment over a snapshot of the live nodes, choose each block's old node
  (its own runtime's segments first, then the other runtimes of the switch at
  the SAME sample rate) and build NOW every processor no old node can stand in
  for (`runtime_node_plan`) — a runtime whose processors are taken plays its
  input unprocessed, so nothing may load in that window; (3) take every
  runtime's processors into one `NodePool` and assemble every next state from
  the chosen nodes plus the prebuilt ones; (4) take every runtime's
  `processing` lock in slot order and commit all of them under those locks —
  a dsp-worker holds its runtime's lock for a whole period, so commits made
  one after another could land a period apart; (5) drop the replaced
  processors after the last lock is released (#670). A failed build hands
  every node back by `instance_serial`. This keeps the node reuse the
  one-runtime chain had (no new VST3 instance where there was one before),
  builds the instances a loop-off switch adds before any output goes dry, and
  never leaves a tail with two writers or none. The one build left inside the
  window is a block whose parameters changed on the same audio processor when
  that processor refuses the in-place retune (#358) — only the processor can
  tell, so it is tried as before; a changed effect or model is built first.

## 4. Out of scope

- Fixed slots for chains whose loop-off pipelines read SEVERAL input devices:
  they keep today's #967 rule (the switch regroups, new streams). Fixed slots
  there would bind one device's input callback (and its dsp-worker) to another
  device's pipeline — an RT-class coupling the LAW forbids. Open question for
  the owner; not a regression (today's behaviour).
- CPU-time isolation on Windows / Linux-cpal: their input callbacks run the
  fan-out inline (`stream_builder_input.rs:114-141` and the I16/U16/I32 arms);
  #980 gives them state isolation only.
- Per-slot timing in the inline paths and pipeline ids in dsp-worker labels.
- Pre-existing, separate issue: tuner, spectrum, looper-record and tone-doctor
  map "input k" to "stream k" (`controller_taps.rs:124-150`,
  `tuner_session.rs:164-193`, `controller_loopers.rs:350-351`,
  `spectrum_session.rs:245-287`, `tone_doctor_live_input.rs:60`).
- Replacing the JACK condvar wake with polling (removes the per-pipeline futex
  wake; changes Orange Pi CPU/latency — only with a measurement).
- JACK chains whose inputs sit on SEVERAL JACK servers (cards): one client
  joins one server, so the pipelines of the other cards get no period
  (pre-existing: before #980 only the first runtime was bound at all).

## 5. Risks

- **R1 #967 window:** between the pipeline key and the fixed slots, a single
  E/S with ≥ 2 outputs and a bound insert regroups on every footswitch press
  (new streams, 2–3 s of silence). No hand-off, PR or validation request before
  the fixed-slot tasks land.
- **R2 Linux window:** until the engine JACK key is deleted, Linux/JACK
  groups per device, so the contract test and every new test that needs
  per-pipeline grouping are gated off Linux+JACK
  (`#[cfg(not(all(target_os = "linux", feature = "jack")))]`) and the Linux
  tree stays green on every push; the key's deletion removes those gates.
- **R3 RT budget:** dsp-workers per device double (each declares 85 % of the
  period at cold start, `dsp_worker.rs:142`) — the #698 overcommit / E-core
  demotion risk. Measured on the real-hardware battery; a regression is a STOP.
- **R4 JACK callback cost:** N period-sized ring copies and N `notify_one`
  futex wakes per period (today 1). Orange Pi CPU: N workers share the big
  cores at SCHED_OTHER nice -10. Owner's ear on the Orange Pi.
- **R5 new sound on JACK:** the second output of an E/S (and mid `Output`s)
  start playing on Linux/JACK — correct by the LAW, audible to the owner.
- **R6 volume/latency:** pinned by the grouped-vs-whole-chain equality tests;
  `volume_invariants_tests.rs` is never edited. Any volume, latency or golden
  failure is a STOP.
- **R7 idle workers:** with a loop on, N-1 idle dsp-workers per head stream
  wake every period and return before any lock.
- **R8 positional keys:** keys are positions in the resolved I/O. Pairing
  depends on the bindings too (two E/S re-paired over the same I/O), so
  `chain_structure_signature` carries `pipeline_slots`, not the slot ids.
- **R9 sync fallback:** `upsert_chain_with_resolved` used to rebuild streams
  only on a stream-signature change; a pipeline change with equal signatures
  (the re-pairing above) left the streams on runtimes the graph dropped. It
  now also rebuilds them when the live runtimes no longer run the chain's
  pipelines, decided before the graph upsert, on both backends.
- **R10 JACK shared channels:** two routes of one chain on the same device
  channels are summed inside the JACK client, with the backend-mix saturation
  guard on exactly those channels. The alternative — a port group per route,
  summed by jackd — changes the client's port shape (#294/#308); only on the
  owner's call.
