# Issue #978, phases 1-2: Windows CI ground truth and packaging

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Get a CI signal that compiles and tests the workspace on Windows, and make the Windows package start on a clean machine and find its bundled data from any working directory.

**Architecture:** A `windows` job in `test.yml` (with `workflow_dispatch`, so the issue branch can run it) is the ground truth for every later Windows change. The packaging fixes live in `scripts/package-windows.ps1` (app-local VC++ runtime plus a dependency check that fails the build) and in a pure, host-testable install-root probe in `infra-filesystem`. `release.yml`'s `build-windows` job runs again on dispatch and tags, but its artifact is not published yet (#816 stays the owner's call).

**Tech Stack:** GitHub Actions (windows-latest, MSVC), Rust stable, cpal `asio` (ASIO SDK + LLVM for bindgen), PowerShell, WiX v3.

**Spec:** issue #978 (audit of `develop` @ 84c1d6b23).

## Global Constraints

- Zero warnings on every target: the Windows job builds with `RUSTFLAGS=-D warnings`.
- No test is skipped, weakened or `#[ignore]`d to get Windows green. A test that is wrong on Windows gets a correct fixture; a behaviour that is wrong on Windows gets a production fix with a red test first.
- Every production file touched declares `//! Responsibility:` and stays under the line cap.
- The main checkout is never touched; all work is in `.solvers/issue-978`.
- `cargo test --workspace` + `cargo build --workspace` + `VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates` before every push.

## Review Focus

- The installed app, started from a terminal in another folder (`openrig-render.exe`, `openrig.exe --mcp`), must find the bundled `assets`/`plugins`/`presets`. Pinned by `install_root_*` tests (Task 3).
- A dev run (`cargo run` on Windows, exe in `target\debug`) must keep using the working directory. Pinned by `install_root_ignores_dir_without_assets` (Task 3).
- A package missing any non-system DLL an exe imports must fail the packaging step, not ship. Pinned by the dependency check in Task 4 (it fails first, before the runtime is copied).
- Tag builds on Windows must survive the bash `source` of `scripts/lib/release-version.sh`. Pinned by the release-version step in the Windows test job (Task 2).
- Enabling `build-windows` must not block the macOS release when Windows fails: `create-release` keeps `needs: [build-macos]` (Task 5).

---

### Task 1: Windows build + test job in CI

**Files:**
- Modify: `.github/workflows/test.yml`

- [ ] **Step 1:** Add `workflow_dispatch:` to `on:` and a `windows` job: checkout (LFS + submodules), stable toolchain, ASIO SDK download exporting `CPAL_ASIO_DIR`, LLVM, `Swatinem/rust-cache@v2`, `cargo build --workspace --all-targets` and `cargo test --workspace`, with `RUSTFLAGS: -D warnings`, `CARGO_INCREMENTAL: 0` and `CARGO_PROFILE_DEV_DEBUG: 0` (PDBs would exhaust the runner disk).
- [ ] **Step 2:** Push, then `gh workflow run test.yml --ref feature/issue-978`. Record every compile error, warning and test failure in the issue. This run is the RED for Tasks 2 and 6: each later task either fixes one recorded failure or proves a finding false.

### Task 2: bash helpers survive the Windows checkout (ci-2)

**Files:**
- Modify: `.github/workflows/test.yml` (windows job)
- Modify: `.gitattributes`

- [ ] **Step 1 (RED):** In the windows job, before the build, add a bash step: `source scripts/lib/release-version.sh && test "$(release_version_from_tag v1.2.3)" = 1.2.3`. Dispatch and confirm that it fails with the `$'\r'` syntax error. If it passes, the finding is refuted: record that and skip Step 2.
- [ ] **Step 2 (GREEN):** Add `*.sh text eol=lf` to `.gitattributes`, then dispatch again and confirm the step passes.

### Task 3: install root found from any working directory (runtime-1)

**Files:**
- Create: `crates/infra-filesystem/src/install_root.rs` (`//! Responsibility: recognises a Windows install directory next to the executable.`)
- Create: `crates/infra-filesystem/src/install_root_tests.rs`
- Modify: `crates/infra-filesystem/src/lib.rs` (module + test wiring)
- Modify: `crates/infra-filesystem/src/asset_paths.rs:82-121` (the Windows branch calls the probe, and the doc comment drops `libs/`)

**Interfaces:**
- Produces: `pub(crate) fn windows_install_root(exe_dir: &Path) -> Option<PathBuf>`, compiled under `cfg(any(target_os = "windows", test))`.

- [ ] **Step 1: Write the failing tests** (`install_root_tests.rs`, run on every OS because the probe is pure):

```rust
use super::windows_install_root;

#[test]
fn install_root_accepts_dir_that_ships_assets() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("assets")).unwrap();
    assert_eq!(windows_install_root(dir.path()), Some(dir.path().to_path_buf()));
}

#[test]
fn install_root_ignores_dir_without_assets() {
    // target\debug in a dev checkout: no assets next to the exe.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("deps")).unwrap();
    assert_eq!(windows_install_root(dir.path()), None);
}

#[test]
fn install_root_does_not_need_the_retired_libs_dir() {
    // Every package since #612: assets + plugins + presets, no libs.
    let dir = tempfile::tempdir().unwrap();
    for d in ["assets", "plugins", "presets"] {
        std::fs::create_dir(dir.path().join(d)).unwrap();
    }
    assert_eq!(windows_install_root(dir.path()), Some(dir.path().to_path_buf()));
}
```

- [ ] **Step 2:** `cargo test -p infra-filesystem install_root` must FAIL (unresolved `windows_install_root`). Then write a stub that returns `None` and see `install_root_accepts_dir_that_ships_assets` fail on its assertion.
- [ ] **Step 3: Implement:**

```rust
pub(crate) fn windows_install_root(exe_dir: &Path) -> Option<PathBuf> {
    exe_dir.join("assets").is_dir().then(|| exe_dir.to_path_buf())
}
```

and in `detect_data_root` replace `if exe_dir.join("libs").exists() { return exe_dir.to_path_buf(); }` with `if let Some(root) = crate::install_root::windows_install_root(exe_dir) { return root; }`.
- [ ] **Step 4:** Run `cargo test --workspace` and confirm it is green. Commit and push.

### Task 4: the package carries the VC++ runtime, and a missing DLL fails the build (pkg-1, pkg-4)

**Files:**
- Create: `scripts/lib/windows-deps-check.ps1` (checks that every DLL imported by a staged exe/dll is either staged or an OS DLL; VC++ runtime names never count as OS DLLs)
- Modify: `scripts/package-windows.ps1`

- [ ] **Step 1 (RED):** Call the check after staging, before the runtime copy exists. Dispatch `release.yml` on the branch (Task 5 enables the job) and confirm that packaging fails and names `VCRUNTIME140.dll` / `MSVCP140.dll`.
- [ ] **Step 2 (GREEN):** Copy `VC\Redist\MSVC\<newest>\x64\Microsoft.VC14*.CRT\*.dll` (found via `vswhere`) into the stage. Dispatch again and confirm the check passes.
- [ ] **Step 3:** Fix the stale MinGW wording in the script and the workflow. The MinGW DLLs stay, because the bundled LV2 DLLs are MinGW builds.

### Task 5: build-windows runs again, and its artifact is not published

**Files:**
- Modify: `.github/workflows/release.yml:334-337`

- [ ] **Step 1:** Replace `if: false` with a comment explaining that the job builds on dispatch and tags, while `create-release` still needs only `build-macos` (#816, #978). Keep the download step commented out.

### Task 6: fix what the Windows run reports

- [ ] **Step 1:** For each failure recorded in Task 1, find the root cause, add a red test when it is behaviour, then fix it. Known upfront: the unused `HostTrait` import (`crates/infra-cpal/src/host.rs:90`) fails under `-D warnings`, and the Unix absolute path in `crates/infra-filesystem/src/lib_tests.rs:85-98`. The HOME-swap tests do not isolate `%APPDATA%`: the decision is made from the actual failure, not in advance.

### Blocked on an owner decision (not in this plan)

- presets-1: where a new, or sidecar-less, project writes chain presets when the bundled folder is read-only (the same question exists for a signed macOS `.app`).
- Publishing Windows assets in releases (#816).
