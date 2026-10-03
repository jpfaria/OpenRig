# Testing

## ⛔ Mandatory TDD — RED-FIRST. No implementation without a test that failed first

**A project law, not a recommendation.** Writing or changing production code
without a test that **failed first** is **forbidden**. A test written after the
implementation (one that passes straight away) proves nothing and rigs the
suite — also forbidden.

**To fix a bug, in this order:**

1. **Interview whoever reported it** — the exact scenario, data, steps,
   expected vs. actual result. Do not guess.
2. **Write a test that reproduces the bug** through the most real path
   possible — **without reading the code for the cause before that.**
3. **Run it and watch it FAIL** (a real RED). Show the failure. If the test
   passes, it did not catch the bug → redo it; or, if it is not a logic bug
   (e.g. Slint rendering, which a unit test does not exercise), **say so
   honestly and stop**.
4. **Only after the RED**, investigate the cause — guided by the failing
   test — and fix until it passes (GREEN).
5. The full suite runs locally only through `./scripts/pre-pr-gate.sh`, before a PR is opened or a branch with an open PR is pushed (see "Full suite").

**Two rounds per delivery, never per micro-step.** A delivery with
several items does not compile once per item:

1. Write ALL the tests of the change — no cargo.
2. ONE round: compile + run those tests, see ALL of them fail (RED).
3. Implement EVERYTHING — no cargo.
4. ONE round: compile + run the same targeted tests, see them pass (GREEN).
5. `cargo fmt --all -- --check` + `VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates`,
   then ONE commit at the end, and push.

No commit and no `cargo build`/`cargo test` between steps. Every `fix(`/`feat(`
commit closes a dev-rules cycle and re-arms the gate, so committing per step
forces a new RED (and a new compile) per step. Exception: while the owner is
validating on his machine, a fix he is waiting for is committed and pushed
right away.

**Local push gate.** Never `cargo test --workspace` or `cargo build --workspace`
locally (10+ minutes on the owner's Mac; CI runs them). Without
`cargo fmt --all -- --check` the `release → main` PR fails on the `fmt` metric.
`./scripts/validate.sh $(git diff --name-only HEAD)` is not a push gate: after
the commit that diff is empty and it always passes — use
`VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates`. A warning counts as
broken (unused import, needless `mut`, dead code).

**Do not search the code for the cause before the test exists and fails.**
Reading the code first produces a biased hypothesis sold as "the cause". The
investigation happens in step 4, driven by the RED.

**Proving a test is not rigged:** revert ONLY the production code to its
pre-fix state (keeping the tests) and run — it must go RED. Restore the
production code afterwards (nothing is lost; it is committed).

**Enforcement:** the gate is the generic hook of the `dev-rules` plugin,
configured in `.dev-rules.json` at the root (Rust globs: `crates/**/src/**`
production, `**/tests/**`/`*_test*.rs`/`*test*.rs` tests). Sentinels in
`.dev-rules/` (never versioned):

- no sentinel → reading AND editing production are blocked (bug discipline).
- `.dev-rules/.mode-feature` → reading production is allowed to plan a
  feature or improvement; editing stays locked to the RED.
- `.dev-rules/.red-first-unlocked` → reading and editing allowed (create it
  only after showing the real RED, step 3 above).

Details and real cases: `.claude/skills/openrig-code-quality/SKILL.md`.

## Coverage

- **Tool**: `cargo-llvm-cov` (install with `cargo install cargo-llvm-cov` + `rustup component add llvm-tools-preview`)
- **Local script**: `scripts/coverage.sh` — writes an HTML report to `coverage/`
- **CI**: the `coverage` job of `.github/workflows/test.yml`, uploaded to Codecov — informative, no gate
- **CI time budget**: the Test Suite runs under `timeout 1500` (25 min) and
  Coverage under a 30-min step limit. Almost all of it is compilation, not tests:
  both jobs restore a dependency cache (`Swatinem/rust-cache`, saved only on branch
  pushes, so PRs read their base branch's), and `cargo-llvm-cov` comes prebuilt.
  A saved cache key is never overwritten, so the Test Suite builds with
  `--keep-going` first: a compile error in a workspace crate must not save a cache
  that is missing dependencies, or every later run compiles them from scratch.
  Instrumentation is what makes long simulations expensive: they run several times
  slower under llvm-cov. A test that simulates minutes
  of audio or sweeps many seeds costs minutes of Coverage.
- **Patch coverage**: `./scripts/patch-coverage.sh [base]` — reproduces locally
  the number `codecov/patch` reports on the PR (`cargo llvm-cov --lcov` crossed
  with `git diff --unified=0 <base>...HEAD`), honouring `codecov.yml`'s
  `ignore:`. The report is reused while the tree does not change (`--fresh`
  forces a new one); `--files` lists what is still missing; `PATCH_COV_OFF=1`
  skips it.

### CI measures the Linux + JACK build

The coverage job runs `cargo llvm-cov --workspace` on Linux, where
`adapter-gui` turns on `infra-cpal/jack` (and through it `engine/jack`). Every
test gated `#[cfg(not(all(target_os = "linux", feature = "jack")))]` — which
includes the controller-driven click tests and most controller harnesses — is
compiled OUT there. It guards macOS, but it produces **no coverage and no
regression signal in CI**. A behaviour that must be guarded in CI needs a test
that runs in both builds: drive the layer below the controller (a
`LiveRuntimeSlot` with runtimes from `build_chain_runtime_state`, or
`update_chain_runtime_state` on the engine side), or build the controller with
`ProjectRuntimeController::for_testing`, which works in both.
`issue_987_slot_handover_tests.rs` and `issue_987_in_place_edit_paths_tests.rs`
are the reference.

### What is deliberately outside the coverage target

Two layers cannot be reached by a test as they stand. They are listed in
`codecov.yml` under `ignore:` so they stop dragging every release PR's patch
score down for a reason nobody can act on. **That list is a debt list: it only
shrinks.** Adding to it means writing the reason here first.

| Layer | Why no test reaches it | The way off the list |
|---|---|---|
| `stream_builder_input.rs`, `stream_builder_output.rs` | Building a cpal stream needs a real `cpal::Device`; `ResolvedInputDevice`/`ResolvedOutputDevice` carry the device itself, so there is nothing to fake. | They ARE exercised — by the real-hardware battery below (`OPENRIG_HW_TESTS=1`), which does not run in CI and emits no coverage. A fake-host seam would move them back in scope. |
| `desktop_app*.rs`, `block_parameter_*.rs`, `settings/paths_seed.rs` | What is left in them is callback registration and window setters, and the repo's law keeps `AppWindow` out of tests. | The `looper_commands` pattern: the closure body becomes a pure function the wiring only calls, and that function gets the test. Then delete the entry. |

Everything else stays in the target. A new file that "cannot be tested" is a
design answer, not a coverage exemption — split the logic out of the wiring.

### Extracting logic out of a callback

Moving logic out of a GUI callback follows one shape:

1. The closure keeps only what a WINDOW does — hide, show, set a property,
   paint a toast.
2. Everything else moves to a file named after what it does
   (`chain_draft_save`, `preset_load`, `block_reorder`), taking the plain
   handles it needs (`Rc<RefCell<Option<ProjectSession>>>`, a `VecModel`) and
   returning a result the callback renders.
3. That function gets the test.

Two things make this work in practice:

- **`VecModel` and `ProjectSession` build fine without a window.** Most of the
  "untestable GUI layer" was never about Slint — it was about logic living
  inside a closure.
- **Never let an extracted function write shared state a test has no business
  touching.** Anything that reaches `config.yaml` takes an explicit path
  (`save_io_bindings_at`, `apply_*_override_at`), and the production entry
  point is that same function called with the real path — so the test drives
  the SAME body against a temp file.

Two functions doing the same job in two files is a duplicate to collapse into
one tested body, not two tests to write.

**What is left uncovered inside an extracted module** is worth naming, because
it is not laziness and it should not be chased with contrived tests:

- **Defensive re-borrows.** A function that drops the session borrow and takes
  it again answers `NotAddressable` on the second read. The session cannot
  vanish between two statements on the GUI thread, so that arm is unreachable —
  and removing it would mean unwrapping.
- **The path-resolving wrappers.** `save_io_bindings`, `apply_*_path` and their
  siblings are two lines: resolve the machine's real `config.yaml` and call the
  `_at` function. Exercising THEM means writing that file. The body they
  delegate to is covered.

## Conventions

- Tests live in a sibling file `<module>_tests.rs`, wired at the end of the module with `#[cfg(test)] #[path = "<module>_tests.rs"] mod tests;`. An inline `#[cfg(test)] mod tests { … }` in a production file fails `validate.sh`. Cross-crate tests go in the crate's `tests/`.
- Names: `<behavior>_<scenario>_<expected>` (e.g. `validate_project_rejects_empty_chains`).
- No external framework. Helpers live in the test module.

## Categories

- **Real-audio integration**: `#[ignore]` (run with `cargo test -- --ignored`)
- **Native DSP**: golden samples with a `1e-4` tolerance, process silence/sine, check non-NaN
- **Native DSP characterisation** (block-delay, `src/dsp_probe.rs`, test-only): deterministic proof that each model does what it is for — echo timing (`peaks`), feedback decay, brightening/darkening (`spectral_centroid`), saturation (`harmonic_ratio`). Non-NaN is not enough: the test measures the trait the model is named after
- **NAM/LV2/IR builds**: `#[ignore]` (external assets)
- **Registry tests** in the block-* crates: iterate over ALL models through the registry
- **Deadline / xrun (timing)**: `#[cfg_attr(debug_assertions, ignore)]` — they
  only make sense in release. `engine/src/audio_deadline_tests.rs` (pipe chains)
  and `engine/tests/issue_670_heavy_rig_deadline.rs` (a heavy rig, per-block
  breakdown) measure the audio thread's per-buffer cost. The cost is dominated
  by NAM inference; stacking several NAM amps saturates the 64-frame budget →
  deadline overrun (xrun) → crackle. The overrun is counted at runtime by
  `ChainRuntimeState::record_callback_load`, fed by the input callback through
  `infra-cpal`'s `callback_load_timing`.

### Looper

The looper is a recorder living on the audio thread, so it is covered at
every layer instead of "it plays, ship it":

| Suite | Proves |
|---|---|
| `engine/src/looper_tests.rs` | the state machine and layer maths — record → play, overdub sums, undo/redo, redo tail dropped by a new recording, ceiling freeze, speed/reverse/decay, export mixdown |
| `engine/src/looper_bank_tests.rs` | the op queue: slot claim, buffer hand-back for unknown uids, mono mixdown, per-looper params |
| `engine/src/looper_runtime_tests.rs` | the callback path: recorded dry input reaches the output, a chain without loopers stays byte-identical silence, a loop survives a runtime rebuild, and one chain's loop never reaches another (invariant #4) |
| `audio_alloc_invariant_tests::looper_record_overdub_and_undo_do_not_allocate` | zero allocation on the audio thread while recording / overdubbing / undoing (invariant #8) |
| `infra-cpal/tests/issue_323_controller_loopers.rs` | ops fan out to every runtime of a chain, each with its OWN buffer |
| `application` dispatcher + `query_loopers` tests | command validation, the footswitch uid-0 sentinel, and the read model every transport shares |
| `adapter-gui/tests/issue_323_looper_wiring.rs` | dispatching alone is dead — a `LooperCommand` must flip the store and the loop's isolated stream, on the bus and with no GUI in the picture |
| `adapter-gui/tests/issue_323_looper_panel_interaction.rs` | real pointer events on the panel: every transport button fires, disabled ones do not, each row reports its own uid |
| `adapter-gui/src/runtime_loopers_tests.rs` | save → reopen round-trip of the wav sidecar (the save dispatched, not called), and that a missing sidecar never blocks opening a project |
| `application/src/local_dispatcher_looper_save_tests.rs` | `SaveProject` exports the loops itself, forgets a cleared loop's stale pointer, and touches nothing when the rig is stopped |
| `adapter-gui/src/runtime_loopers_826_tests.rs` | the same round-trip on a RIG project, reopened FROM DISK — record → close → reopen, plus the same after a chain rename and after a waveform edit |
| `infra-cpal/tests/issue_323_looper_hw.rs` (`OPENRIG_HW_TESTS=1`) | the REAL stack: record + 7 overdubs + undo/redo/clear on live CoreAudio streams at buffer 64 cost **zero** xruns / underruns |

A persistence round-trip has to reopen the way the app reopens: read the file
back with `project_ops::load_rig_and_project` and restore into a FRESH
controller. `runtime_loopers_tests` does not — its session carries `rig: None`
and it reuses the in-memory project — so a bug that only shows on a real
reopen walks straight past it. The app's project is a rig: what hits
disk is built from `RigProject`, not from `Project`, so anything stamped onto a
chain AFTER the rig capture never reaches the file. `runtime_loopers_826_tests`
is the version that reopens for real; prefer it as the template.

Neither needs an audio device: `ProjectRuntimeController::for_testing_with_sample_rate`
builds a controller with a real `LooperStore` and opens nothing, so record →
edit → save → reopen → restore runs headless in a unit test.

The hardware test builds its rig **in the test** instead of loading a fixture
preset: the shipped presets reference the owner's NAM/LV2 capture library, so
on a machine without it every block is dropped, the chain never comes up, and
the counters read zero for a runtime that does not exist — a vacuously green
measurement. It asserts the chain is live before measuring, and drives
`poll_pending_rebuilds` the way the app's timer does, because the cold
activation is asynchronous.

## Full suite

The whole workspace suite runs in CI (the `Test Suite` job of
`.github/workflows/test.yml`, on Linux). While working, run only the targeted
tests of your change (`cargo test -p <crate> <filter>`).

Before `gh pr create`, and before every push to a branch whose PR is open, run
`./scripts/pre-pr-gate.sh` on the committed HEAD. It runs what CI runs (fmt,
the whole-repo static checks, the workspace tests) and stamps the commit it
passed on. Tests run through `cargo nextest run --workspace` (every test binary
in parallel) plus `cargo test --workspace --doc`, in CI and in the gate, which
falls back to `cargo test` when nextest is not installed
(`brew install cargo-nextest`); the Claude hook `.claude/hooks/pre-pr-gate-guard.sh` denies the PR or
the push unless HEAD carries the stamp, so commit first and push in a command of
its own. Pushes to a branch with no PR are not gated. It runs on macOS: a
failure that exists only on Linux (a `cfg(target_os = "linux")` path, the JACK
backend) still shows up in CI only.

## Real-hardware battery

`crates/infra-cpal/tests/issue_670_cab_swap.rs`,
`crates/infra-cpal/tests/issue_670_real_streams_no_xruns.rs`,
`crates/infra-cpal/tests/issue_698_pitch_shifter_live.rs`,
`crates/infra-cpal/tests/issue_698_owner_64_dual_chain.rs` and
`crates/infra-cpal/tests/issue_328_dual_amp_split.rs` open the REAL
audio interface (CoreAudio streams, the owner's presets and DI takes) and
assert real-time deadlines through the engine's own xrun/underrun counters.
They are the full-fidelity reproduction harness for a crackle on cab swaps
and for a multi-chain RT-budget overcommit (shared helpers live in
`tests/hw_harness/`). The `issue_698_*` owner-recipe tests additionally need the
real capture library via `OPENRIG_OWNER_PLUGINS=<plugins/source>`.
The dual-amp split test plays a Split → Mix chain (amp A hard left, amp B
hard right) and prints its peak load next to a single amp's.

They are only meaningful on an otherwise idle machine, so they are gated by
an environment variable and return immediately (with a loud notice on
stderr) when it is absent — they never fail under the parallel workspace
suite or the quality gate for reasons unrelated to the app. **Any agent or
contributor can (and should) enable them when validating audio-path
changes:**

```sh
OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
    --test issue_670_cab_swap --test issue_670_real_streams_no_xruns \
    --test issue_698_pitch_shifter_live --test issue_698_owner_64_dual_chain \
    --test issue_328_dual_amp_split
```

Requirements: macOS, a real input/output interface connected (the suite
looks for the Scarlett by name), an idle machine, and ~12 minutes. The
tests serialize access to the physical device across processes via a lock
file.

The same gate covers the metronome's runtime doors
(`crates/adapter-gui/src/metronome_runtime_tests.rs`): starting the
click means `find_output_device_by_id` → `host.output_devices()`, so those
tests enumerate the machine's real interfaces and one of them opens a (silent)
output stream. They are seconds, not minutes:

```sh
OPENRIG_HW_TESTS=1 cargo test -p adapter-gui --lib runtime_lifecycle::metronome_tests
```

What the ORDER those doors write the generator's `enabled` flag in — the click
is marked playing only once its stream is proven open — is pinned headless in
`crates/adapter-gui/src/runtime_pipelines_tests.rs` and runs in the normal
suite.

`crates/infra-cpal/tests/issue_127_metronome_runtime.rs` is the same gate one
layer down, on the controller's own doors, and needs a real output device for
the same reason. It pins that an open click keeps `is_running()` true — the
predicate the chain-teardown doors drop the WHOLE controller on, so an
uncounted click dies when a chain is enabled and disabled again — and that an
output change the device refuses leaves a click that was already playing
untouched, rather than closing its stream and leaving every reader saying
"playing" over silence. Seconds, and never audible (the generator is muted and
never enabled):

```sh
OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --test issue_127_metronome_runtime
```

**Mid-chain ports** get their own three files in the same battery. They
need no player and make no noise: a **loopback device** (BlackHole 2ch) stands
in for the second interface, a DI loop (or a tone written by the test) is the
source, and the test opens the loopback's input to hear what actually arrived.
Each one carries a **control case** — the same rig measured where it is known to
work — so a silent run can never be blamed on the measuring path.

```sh
OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
    --test issue_85_mid_output_reaches_its_device \
    --test issue_85_mid_input_reaches_the_tail \
    --test issue_85_chain_toggle_keeps_audio -- --nocapture
```

Requirements: BlackHole 2ch installed, plus one other interface. Two things a
headless run needs and the GUI does for you: install the binding registry
**before** `start` (`start_with_io_bindings`), then **poll pending
activations** — the cpal streams are created on the polling thread, so without
`poll_pending_rebuilds()` nothing ever opens and every measurement reads zero.

**Counting underruns is not listening.** A tap can hold its last frame, alias or
drift and still report zero underruns — a false positive. `issue_85_mid_output_other_rate_real` keeps every frame the
device popped and measures how much of it is NOT the tone, per short window (so
a slow clock trim is not counted as distortion), against the chain's OWN tail
captured in the same run — same processing, no conversion. Any new audio-path
claim needs that shape of oracle, not a counter.

`issue_85_owner_interfaces_tap` runs on two REAL interfaces instead of the
loopback (which shares the machine's clock), which is where a cross-rate tap
actually misbehaves, and reports the peak callback load alongside the counters.

The teardown / whole-graph doors need no hardware either. At the
dispatcher level, `crates/application/src/local_dispatcher_runtime_doors_tests.rs`
drives a spy `RuntimeControl` and pins that `StopProjectRuntime` and
`CloseProject` stop the rig, that `SaveAudioSettings` rebuilds the whole graph
(and surfaces a refusal as a dispatch error), and — the isolation pin — that
`RemoveChain` touches ONLY the deleted chain: the survivor is neither re-synced
nor named, and the rig-wide stop never fires. At the GUI level,
`crates/adapter-gui/src/runtime_lifecycle_control_tests.rs` drives the same
commands against a device-less controller and asserts the controller is dropped
and the engine rate goes back to the reference, plus that the whole-graph
rebuild never CREATES a controller on a stopped rig.

The poll tick's doors (`crates/adapter-gui/src/runtime_health_tests.rs`) need no hardware: they drive a device-less controller
(`ProjectRuntimeController::for_testing`) whose rebuilds resolve their
endpoints from the binding registry. They pin that a finished rebuild is
INSTALLED by the write door (and only into the chain that asked for it), that a
`BlockError` the audio thread posted surfaces through the read door naming its
chain, and that the read drains — so nobody promotes it to a shared query.

`crates/adapter-gui/src/chain_row_seams_tests.rs` pins the meter tick's three
per-chain doors against a device-less controller: a hosted chain reports its own
runtime state (live, with its own xrun/underrun counters) and its own DI state,
a chain with no runtime on a hosted frontend reads `live: false` rather than
`None`, a stopped rig answers neither, and the looper reconcile gives the
project's looper its slot while leaving a sibling chain's store empty.

`crates/adapter-gui/src/block_stream_read_tests.rs` pins the block-diagnostic
read's two states against a device-less controller: a hosted runtime always
answers (an empty table for a block that publishes nothing), and a stopped rig
answers `None` — the distinction the panel uses to decide between "go inactive"
and "keep showing what you have".

The subscription seam is tested on both sides and needs no hardware either.
`crates/application/src/audio_taps_tests.rs` pins the CONTRACT — a tap that
carries no PCM is still a complete implementation, a frontend that hosts no
audio subscribes to nothing, and a `TapPoint` is never satisfied by a stream
index alone. `crates/adapter-gui/src/runtime_taps_tests.rs` drives the GUI's
implementation against a device-less controller: the reduced reading equals the
peak of the window the audio callback pushed, the raw window honours its cap, a
subscription never hears a sibling chain, and one multi-channel subscription
keeps its channels apart.

## Structural invariant: the UI may not name the audio backend

`crates/adapter-gui/src/no_infra_cpal_in_wiring_tests.rs` asserts on the crate's
own SOURCE rather than on behaviour, because what it protects is a boundary no
runtime test can see: a module that reaches `infra_cpal::ProjectRuntimeController`
directly still works in the GUI while doing nothing over MCP/gRPC. Three
assertions:

- no `adapter-gui` module outside an explicit allowlist names the backend — a
  wiring module reaches the audio through one of the three seams (`Command` +
  `RuntimeControl` for writes, `LiveSource` for reads, `AudioTaps` for
  subscriptions);
- no allowlisted module has STOPPED naming it, so the list can only shrink —
  a ratchet, not a graveyard;
- `sync_live_chain_runtime` has exactly ONE caller, the module that owns the
  runtime.

It walks `src/` recursively (a flat scan would let `settings/audio.rs` past) and
strips `//` comments before matching, in both directions, so a comment that
names the identifier never counts as a call. The ledger in
that file justifies every allowlist entry; see `docs/architecture.md` → "The
guard: the UI may not name the backend".

```sh
cargo test -p adapter-gui --lib no_infra_cpal
```

## Real-plugin VST3 battery

Tests that load a real catalog VST3 (ChowCentaur) are gated on
`OPENRIG_TEST_VST3_DIR` — the plugins `vst3/` dir (e.g.
`<OpenRig-plugins>/plugins/source/vst3`) — and skip cleanly when it is unset,
so CI and the parallel suite stay green. They must run single-threaded
(`--test-threads=1`): JUCE plugins refuse *concurrent* instantiation.

- `crates/vst3-host/tests/issue_776_catalog_vst3.rs` — discovery, load and
  processing.
- `crates/vst3-host/tests/issue_780_catalog_params.rs`,
  `issue_780_controller_reaches_dsp.rs`, `issue_780_param_affects_dsp.rs`,
  `issue_780_live_context_params.rs` — the catalog exposes the real
  parameters, a value set on the controller reaches the DSP and is audible,
  and a live instance resolves its params through its own context.
- `crates/project/tests/issue_780_vst3_knobs.rs` — a catalog VST3 produces a
  parameter schema (knobs).
- `crates/infra-cpal/tests/issue_779_vst3_live_param_no_reinstantiate.rs` — a
  param change on a live chain does not re-instantiate the plugin.
- `crates/engine/tests/issue_938_vst3_package_id_renders.rs` and
  `crates/adapter-render/tests/issue_938_render_resolves_vst3.rs` — a VST3
  addressed by its package id builds and renders.

```sh
OPENRIG_TEST_VST3_DIR=<OpenRig-plugins>/plugins/source/vst3 \
    cargo test -p vst3-host -p project -- --test-threads=1
```
