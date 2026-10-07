---
name: openrig-tooling
description: Use when you need to build, package, sign, render, or test OpenRig — running any of its scripts/tools (macOS .dmg, Linux .deb/Orange Pi image, headless Slint PNG render, coverage, the real-hardware test battery) and you need the exact command plus the non-obvious prerequisites.
---

# OpenRig Tooling Runbook

## Overview

Index of how to actually *run* OpenRig's tooling, with the gotchas that aren't
obvious from the script names. The per-script detail lives in `docs/scripts.md`
and each script's header comment + `--help`; this skill is the fast path and
fills the gaps those don't cover.

**Golden rule (CLAUDE.md law 1):** an agent never builds in the user's main working
tree. Work in a `.solvers/issue-N` clone (or, for a throwaway artifact, a temp
clone outside the repo). Clone the branch you actually want — a build from a
stale clone ships stale code.

## When to use

- "build me a mac app / .dmg", "package", "make a release artifact"
- "render this screen / check the layout" (Slint → PNG, no GUI)
- "run the tests" including the ones that open the real audio interface
- ".deb for the Orange Pi", flashing an SD image
- coverage report

Not for: editing code (see `openrig-code-quality`), or UI design work (see
`slint-best-practices` + `claude-plugin:ux-ui`).

## Quick reference

| Goal | Command (from repo root) |
|---|---|
| macOS universal .dmg | `OPENRIG_PLUGINS_DIR=<plugins> ./scripts/package-macos.sh <ver>` |
| macOS build + install to /Applications | `OPENRIG_PLUGINS_DIR=<plugins> ./scripts/install-macos-local.sh [ver]` (dev; builds via packager, quits+replaces+launches) |
| Linux .deb (arm64+amd64) | `./scripts/build-deb-local.sh` (Docker Desktop running) |
| Orange Pi SD image | `./scripts/build-orange-pi-image.sh --local-deb output/deb/openrig_*_arm64.deb` |
| Headless Slint → PNG | `cargo run --manifest-path tools/slint-render/Cargo.toml --release -- <file.slint> <Component> <out.png> [w] [h]` |
| Unit/integration tests | `cargo test --workspace` (add `-- --ignored` for the `#[ignore]` audio tests) |
| Real-hardware battery | `OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release` (idle machine, real interface) |
| Coverage HTML | `./scripts/coverage.sh` |

## macOS .dmg — the parts that bite

`scripts/package-macos.sh <ver>` builds a universal (arm64+x86_64) bundle,
ad-hoc signs inside-out, and emits `dist/OpenRig-<ver>-macos-universal.dmg`.

- **The bundled LV2/VST3 tree is versioned here** under `plugins/source/` (#1093),
  as Git LFS objects (~600 MB). A clone made without them has only pointers and
  the bundle ships broken plugins — pull them first with
  `git lfs pull --include="plugins/source/**"`. NAM/IR captures never ship; they
  live in the private OpenRig-plugins repo (`config.yaml` → `paths.plugins_path`).
- **NAM sources come from Git LFS, not a submodule:** the `nam` build
  unpacks `deps/NeuralAmpModelerCore.tar.gz` into `deps/NeuralAmpModelerCore/`.
  A clone made without LFS has only a pointer there and the cmake build of
  `libnam_wrapper.dylib` aborts the packager — fix with `git lfs install &&
  git lfs pull`. `git submodule update` does nothing (there are no submodules).
- **Signing is ad-hoc only** (`codesign --sign -`): it downgrades Gatekeeper
  from "damaged" to "unidentified developer" (right-click → Open). No Developer
  ID / notarization exists in this repo.
- **Every Mach-O must be signed** — the GUI `openrig`, the console binaries
  listed in `scripts/lib/console-binaries.tsv` (`openrig-console`,
  `openrig-console-rig`, `openrig-render`), `libnam_wrapper.dylib`, and each
  nested plugin `.dylib`. If a new binary is added to `Contents/MacOS/` and the
  packager doesn't sign it, the `codesign --verify --deep --strict` gate fails
  with "code object is not signed at all / In subcomponent: …". Fix the script
  (or sign the new binary inside-out), don't ship the unverified bundle.
- Needs: `rustup` with both `*-apple-darwin` targets, Xcode CLI tools
  (`lipo`/`sips`/`iconutil`/`install_name_tool`/`hdiutil`/`codesign`), cmake.

## Headless Slint render (undocumented elsewhere)

`tools/slint-render` renders a `.slint` component to PNG via `slint-interpreter`
+ software renderer — no display server. It is its **own workspace** (the root
`Cargo.toml` excludes it), so you MUST use `--manifest-path tools/slint-render/Cargo.toml`
(a plain `-p` from root won't find it). Default size 900×900; exit 2 = bad args,
exit 1 = compile error / component not found. Colors are Rgb565 approximations.
For an app component, write a standalone `.slint` mockup (root `inherits Window`,
fixed size, fake data) and render that. **`docs/render.md` is a different tool**
(`openrig-render`, the offline audio renderer) — not for screens.

## Tests

- `cargo test --workspace` — normal suite; `#[ignore]` audio/NAM/LV2/IR tests
  skipped. Add `-- --ignored` to include them.
- **Real-hardware battery:** `OPENRIG_HW_TESTS=1` gates tests that open the real
  CoreAudio interface (idle machine, real interface connected, ~release only —
  timing tests are `#[cfg_attr(debug_assertions, ignore)]`). See `docs/testing.md`.
  Without the env var they no-op; they never fail the normal run.

## Opening the app yourself (agent test)

Before handing the owner a checklist, the agent opens the solver's app and tests
everything that does not need his ear (log lines, `openrig://routes`,
`openrig://meters`, restarts, isolation between chains). Recipe:

1. **Wait for any other OpenRig to close (CLAUDE.md law).** Poll, never kill:
   `while pgrep -f 'target/(debug|release)/adapter-gui|OpenRig.app/Contents/MacOS/openrig' >/dev/null; do sleep 30; done`.
   Same for your own probes or recorders that open the interface.
2. **Isolate with `HOME`, not only `--config`.** The session loader reads the I/O
   bindings and device settings from the default config path
   (`$HOME/Library/Application Support/OpenRig/config.yaml`), whatever `--config`
   says. Write the test config there under a scratch `HOME` and pass the same path
   to `--config`. The owner's config and project are never read for writing.
3. **Project format = the branch's.** `version:` must not exceed the branch's
   `PROJECT_FORMAT_VERSION` (`crates/project/src/rig.rs`), or the app falls back
   to the launcher. Every input loads OFF.
4. **Virtual device, no speakers.** BlackHole 2ch (`coreaudio:BlackHole2ch_UID`,
   48000 Hz, 64 frames, matching its current rate so nothing is changed). It loops
   output back to input, so give the preset `volume: 0`. Feed it from a Python
   `sounddevice` stream (BlackHole as output device).
5. **Launch the built binary directly**, not via `nohup` or `env`: macOS strips
   `DYLD_*` when it runs a system binary, and the binary needs the NAM wrapper.
   ```
   HOME=<scratch>/home DYLD_FALLBACK_LIBRARY_PATH=$PWD/$(ls -td target/debug/build/nam-*/out/lib | head -1) \
   OPENRIG_PLUGINS_ROOT=<plugins_path> RUST_LOG=info target/debug/adapter-gui \
     --config "<scratch>/home/Library/Application Support/OpenRig/config.yaml" \
     --project <scratch>/project.yaml --mcp=127.0.0.1:4124 > <scratch>/app.log 2>&1
   ```
   Use a port other than 4123 (the owner's app).
6. **Drive and read over MCP:** `toggle_chain_enabled {"chain":"rig:<input-id>"}`
   to start a chain; `openrig://routes` (callbacks, underruns per route) and
   `openrig://meters` (input/output dBFS per chain); the log for warnings.
7. **Close everything:** kill the app and the feeder, confirm the device's rate is
   unchanged (`system_profiler SPAudioDataType`) and the project file untouched.

## Linux .deb / Orange Pi

`./scripts/build-deb-local.sh` cross-compiles inside the same Debian Docker
container CI uses (Docker Desktop must be running) → `output/deb/openrig_*_arm64.deb`.
Never compile on the board; only arm64 goes to the Orange Pi. Flags: `--arch`,
`--clean` (on `E0460`/`E0463` cache corruption), `--nuke`. Board OS changes
mirror into `platform/orange-pi/` (`docs/hardware/orange-pi-deploy.md`).

## Common mistakes

- Building from a clone taken **before** the fix you need was merged → run from
  an up-to-date clone of the target branch (verify HEAD).
- Forgetting `cargo clean` in a `.solvers/` workspace after a merge / multi-crate
  struct change → stale `target/` causes `E0460`/`E0463`/ICE (`docs/scripts.md`).
- Bundling a `.dmg` before `git lfs pull --include="plugins/source/**"` → the
  plugin binaries are LFS pointers and no LV2/VST3 effect loads.
