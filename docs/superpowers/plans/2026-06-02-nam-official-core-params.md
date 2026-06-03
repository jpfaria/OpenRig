# NAM Official Core — restore noise gate / EQ / IR params Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make ALL NAM block knobs actually affect the sound — `input_db`, `output_db`, `noise_gate.*`, `eq.bass/middle/treble`, and `ir_path` — by restoring the official `NeuralAmpModelerCore` + the `cpp/` C++ wrapper that applied them, instead of the inference-only `NeuralAudio` engine that dropped them. **input/output are forwarded through the wrapper config too** (not a separate Rust-side gain), so the whole chain runs in one place: input gain → model → noise gate → tone stack (EQ) → IR → output gain.

**Architecture:** Re-adopt the pre-`ece3a1474` design: a C++ wrapper (`cpp/nam_wrapper.cpp`) builds an official `nam::DSP` model and chains the official AudioDSPTools `dsp::noise_gate` + `dsp::ImpulseResponse` and the local `openrig::BasicNamToneStack` (EQ), all driven by a `NamPluginConfig` struct. Rust FFI calls `nam_create / nam_process / nam_destroy`. The Rust schema/params (`NamPluginParams`) already match the old `NamPluginConfig` 1:1 — they are leftovers from this wrapper.

**Tech Stack:** C++17, CMake, `deps/NeuralAmpModelerCore` (submodule, includes AudioDSPTools), Eigen, Rust FFI (`crates/nam`).

**Source of truth for restored files:** commit `ece3a1474~1` (the parent of "refactor: replace NeuralAmpModelerCore with neural-amp-modeler-lv2 engine"). Retrieve exact content with `git show ece3a1474~1:<path>`.

---

## ⚠️ Risk gate — perf on Orange Pi (do Task 1 FIRST, decide before the rest)

The engine was swapped AWAY from `NeuralAmpModelerCore` in `ece3a1474` **for speed** ("RTNeural backend, faster"). Restoring the official core may regress CPU/latency — a CLAUDE.md invariant (#1 latency, #7 audio-thread CPU, #3 no xruns), worst on aarch64/Orange Pi. **Task 1 measures this before any wiring.** If it regresses real-time on the target, STOP and switch to the hybrid fallback (keep the fast `NeuralAudio` engine, wrap it with the same AudioDSPTools `NoiseGate`/`ImpulseResponse` + `BasicNamToneStack` modules) — same official DSP, no engine swap.

---

## File Structure

- `deps/NeuralAmpModelerCore` — submodule (official core + AudioDSPTools), restored.
- `cpp/nam_wrapper.h` / `cpp/nam_wrapper.cpp` — C API wrapper (`NamPluginConfig`, `nam_create/process/destroy`).
- `cpp/nam_tone_stack.h` / `cpp/nam_tone_stack.cpp` — `openrig::BasicNamToneStack` (bass/middle/treble EQ).
- `cpp/CMakeLists.txt` — builds the wrapper + links the core.
- `crates/nam/build.rs` — CMake build of `cpp/` + link directives (replaces the `NeuralAudioCAPI` build).
- `crates/nam/src/processor.rs` — FFI: replace the `NeuralAudio` extern block + `NamProcessor` with the `nam_create/nam_process/nam_destroy` path; map `NamPluginParams` → `NamPluginConfig`.
- `crates/engine/tests/issue_612_nam_eq_applied.rs` — restore the red→green EQ test (was reverted in `d6b17a1f`).
- `crates/engine/tests/issue_612_nam_noise_gate.rs` — gate test.
- `libs/nam/<platform>/` — prebuilt binaries: rebuilt per platform (host here; CI for the rest).

---

### Task 1: Perf spike — measure official core vs current engine (RISK GATE)

**Files:**
- Temp: a throwaway bench under `crates/nam/examples/` or a one-off `cargo bench`-style test.

- [ ] **Step 1: Restore the submodule on a scratch branch**

```bash
git submodule add https://github.com/sdatkinson/NeuralAmpModelerCore.git deps/NeuralAmpModelerCore || true
git -C deps/NeuralAmpModelerCore checkout $(git show ece3a1474~1:.gitmodules >/dev/null 2>&1; echo main)
git submodule update --init --recursive deps/NeuralAmpModelerCore
```

- [ ] **Step 2: Build the official model + process a buffer, time it vs the current `NeuralAudio` Process**

Measure per-1024-frame process time at 48 kHz for the fixture model (`crates/engine/tests/fixtures/plugins/nam/marshall_plexi/captures/angus_nano.nam`) on:
  - this host (macOS), and
  - **the Orange Pi (aarch64)** — the binding target.

- [ ] **Step 3: Decision**

Expected: official core slower. If the aarch64 per-block time stays under the real-time budget (`budget_us = nframes * 1e6 / 48000`, with margin for the rest of the chain), proceed to Task 2. If NOT, STOP — switch to the hybrid (keep `NeuralAudio` engine, add the AudioDSPTools `NoiseGate`/`ImpulseResponse` + `BasicNamToneStack` around it). Record the numbers in issue #612.

---

### Task 2: Restore the C++ wrapper + tone stack + CMake from history

**Files:**
- Create (from history): `cpp/nam_wrapper.h`, `cpp/nam_wrapper.cpp`, `cpp/nam_tone_stack.h`, `cpp/nam_tone_stack.cpp`, `cpp/CMakeLists.txt`
- Submodule: `deps/NeuralAmpModelerCore`

- [ ] **Step 1: Restore the cpp/ files exactly as they were**

```bash
git checkout ece3a1474~1 -- cpp/nam_wrapper.h cpp/nam_wrapper.cpp \
  cpp/nam_tone_stack.h cpp/nam_tone_stack.cpp cpp/CMakeLists.txt
```

- [ ] **Step 2: Confirm the C API surface (no edits expected)**

`cpp/nam_wrapper.h` must declare:

```c
typedef struct NamPluginConfig {
  const char* model_path_utf8;
  const char* ir_path_utf8;
  float input_db;
  float output_db;
  float noise_gate_threshold_db;
  float bass;
  float middle;
  float treble;
  unsigned char noise_gate_enabled;
  unsigned char eq_enabled;
  unsigned char ir_enabled;
} NamPluginConfig;

void* nam_create(const NamPluginConfig* config);
void  nam_destroy(void* handle);
void  nam_process(void* handle, const float* input, float* output, int nframes);
```

- [ ] **Step 3: Re-add the submodule (pin the SHA used pre-swap if recoverable from history)**

```bash
git submodule add https://github.com/sdatkinson/NeuralAmpModelerCore.git deps/NeuralAmpModelerCore
git submodule update --init --recursive deps/NeuralAmpModelerCore
git add .gitmodules deps/NeuralAmpModelerCore
```

- [ ] **Step 4: Commit the restored C++ side**

```bash
git add cpp/
git commit -m "feat(#612): restore official NAM C++ wrapper (gate/EQ/IR) + NeuralAmpModelerCore submodule"
```

---

### Task 3: Restore the CMake build in build.rs

**Files:**
- Modify: `crates/nam/build.rs`

- [ ] **Step 1: Inspect the old build.rs to copy the cmake invocation**

```bash
git show ece3a1474~1:crates/nam/build.rs
```

- [ ] **Step 2: Replace the current `NeuralAudioCAPI` build path with the old `cpp/` cmake build**

The old build.rs (a) ran `cmake` on `cpp/`, (b) emitted `cargo:rustc-link-search` + `cargo:rustc-link-lib` for the wrapper + core, (c) copied the artifact to `libs/nam/<platform>/`. Keep the existing prebuilt-first fast path but point it at the new wrapper artifact name.

- [ ] **Step 3: Build**

Run: `cargo build -p nam`
Expected: compiles `cpp/` via cmake, links the wrapper, no errors.

- [ ] **Step 4: Commit**

```bash
git add crates/nam/build.rs
git commit -m "build(#612): build the NAM C++ wrapper (official core) via cmake"
```

---

### Task 4: Rewrite the Rust FFI to use the wrapper

**Files:**
- Modify: `crates/nam/src/processor.rs` (replace the `NeuralAudio` extern block + `NamProcessor::new`/`process_block`/`process_sample`)

- [ ] **Step 1: Reference the old processor.rs FFI**

```bash
git show ece3a1474~1:crates/nam/src/processor.rs
```

- [ ] **Step 2: Replace the extern block + processor**

Declare `extern "C"` `nam_create(*const NamPluginConfig) -> *mut c_void`, `nam_process(*mut c_void, *const f32, *mut f32, c_int)`, `nam_destroy(*mut c_void)`. Build a `#[repr(C)] NamPluginConfig` from `NamPluginParams` (CString for the two paths) in `NamProcessor::new`; call `nam_process` in `process_block`/`process_sample`; `nam_destroy` in `Drop`. Keep the `MODELS_LIVE`/`MODELS_CREATED` counters (#588) and `soft_clip` (#496) on the output.

- [ ] **Step 3: Build the workspace**

Run: `cargo build -p nam -p engine`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/nam/src/processor.rs
git commit -m "feat(#612): NAM FFI forwards gate/EQ/IR params to the official core wrapper"
```

---

### Task 5: Restore the red→green tests (EQ + gate now applied)

**Files:**
- Create: `crates/engine/tests/issue_612_nam_eq_applied.rs` (restore from `d672ef78~1`'s version or rewrite)
- Create: `crates/engine/tests/issue_612_nam_noise_gate.rs`

- [ ] **Step 1: Restore the EQ test**

```bash
git show d672ef78:crates/engine/tests/issue_612_nam_eq_applied.rs > crates/engine/tests/issue_612_nam_eq_applied.rs
```

- [ ] **Step 2: Run it — now it must PASS (EQ applied by the official tone stack)**

Run: `cargo test -p engine --test issue_612_nam_eq_applied`
Expected: PASS (`nam_eq_bass_vs_treble_changes_the_output`).

- [ ] **Step 3: Add a gate test (sub-threshold collapses; above-threshold not strangled)**

Use the same fixture harness; set `noise_gate.enabled=true`, `noise_gate.threshold_db` and feed sub- vs above-threshold sines; assert the sub-threshold tail collapses and the above-threshold note is preserved.

- [ ] **Step 4: Run + commit**

```bash
cargo test -p engine --test issue_612_nam_eq_applied --test issue_612_nam_noise_gate
git add crates/engine/tests/issue_612_nam_eq_applied.rs crates/engine/tests/issue_612_nam_noise_gate.rs
git commit -m "test(#612): NAM gate + EQ params change the output via the official core"
```

---

### Task 6: Regression + cross-platform binaries + docs

- [ ] **Step 1: Regression** — `cargo test -p nam --lib` and the engine NAM tests (`nam_output_gain_no_clip`, `nam_loudness_measure`, `issue_588_mono_nam_single_instance`) green.
- [ ] **Step 2: Rebuild the 5 prebuilt libs** via `scripts/build-lib-internal.sh` / CI for macOS-universal, linux-x86_64, linux-aarch64, windows-x64, windows-arm64; commit updated `libs/nam/<platform>/`.
- [ ] **Step 3: Docs** — `docs/blocks-catalog.md` NAM params: gate/EQ/IR now applied via the official core; note the engine change + the perf trade-off; update `docs/user-guide/installation.md` lib note.
- [ ] **Step 4: Validate on hardware** — user confirms gate/EQ/IR audibly work and the Orange Pi has no new xruns.

---

## Self-Review

- **Spec coverage:** `input_db` + `output_db` (forwarded in `NamPluginConfig`, applied as `input_gain`/`output_gain` in the wrapper — Task 4), gate (Task 4/5), EQ (Task 4/5), IR (`ir_path` restored via the wrapper, Task 2/4). All six knob groups land in one config struct. ✓
- **Perf invariant:** Task 1 risk gate + Task 6 step 4 hardware validation. ✓
- **Types:** `NamPluginConfig` fields match the restored header and the existing `NamPluginParams`. ✓
- **Open item:** the exact `NeuralAmpModelerCore` submodule SHA used pre-swap — recover from `ece3a1474~1` git history if the submodule gitlink is present; else pin a known-good release tag.
