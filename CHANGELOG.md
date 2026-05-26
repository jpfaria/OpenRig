# Changelog

OpenRig releases on a rolling `0.1.0-dev.N` cadence while the surface area
stabilises toward the first `0.1.0` tag. Each section lists the user- and
contributor-visible changes that landed in that release, grouped by area.
Issue links (`#NNN`) point to the public tracker with the full motivation,
repro, and trade-off discussion behind the change.

## Unreleased

### Added
- **`openrig-render` — headless offline render console.** New
  standalone binary in `crates/adapter-render`, plus a new
  `engine::offline::render_chain` driver that reuses the same
  `RuntimeProcessor::process_buffer` as the realtime callback — chains
  render byte-identical between offline and live for the same input.
  Deterministic, atomic write (`<output>.tmp` + rename). Drives the
  audio-validation loop for the `openrig-tone-analyzer` skill
  (OpenRig-claude#8). Single-chain, no I/O blocks, no MIDI/automation
  replay — multi-chain rendering remains out of scope. The GUI binary is
  unaffected (`openrig-render` is its own console, not a flag on
  `adapter-gui`). See `docs/render.md` (#552).

## v0.1.0-dev.23 — 2026-05-25

The "project & rig" release: the project-level I/O + per-input preset bank
architecture (#436 umbrella) lands end-to-end alongside several audio
correctness fixes, instant chain/block toggling, and the first cut of the
MCP server.

### Audio
- **Mono output no longer silences hardware-stereo USB interfaces.** The
  CPAL stream selector used to downsize a stereo device (Scarlett 2i2,
  etc.) to 1 channel when the user picked `OutputBlock.mode = Mono` with
  a single channel, which CoreAudio routes nowhere on macOS. The selector
  now opens the device at AT LEAST its native channel count and the
  engine handles per-channel routing inside the interleaved buffer
  (#516).
- **NAM loudness calibration restored end-to-end.** Manifests carry
  `output_gain_db`, the engine reads it, the limiter holds 0 dBFS, and
  hot captures (CPM 22 class at +18 dB) no longer clip — loud and clean
  (#491, #496, #514).
- **Native pitch shifter no longer drops notes** on Linux/macOS (#488).
- **Multi-interface input chains** route N isolated runtimes to one
  shared physical output, summed at the backend (#17, #451).

### Editor / Project / Presets
- **PROJECT / CHAIN / PRESET / SCENE hierarchy works.** Adding a chain
  auto-creates one preset + one scene; the `+` buttons in the chain row
  actually wire to the dispatcher; preset rename round-trips through
  save/reload; the whole tree persists end-to-end (#504, #449, #450).
- **Project-level I/O + per-input preset banks (rig architecture).** A
  rig is a set of named input sources, each with its own preset bank;
  switching presets per input is independent of the others. Foundational
  for stage usage (#436 umbrella, #451, #453).
- **Scenes per preset** with new-stream + crossfade + tail spillover —
  scene switches don't cut delays or reverbs mid-decay (#454).
- **Preset round-trip is faithful.** Load no longer duplicates I/O
  blocks; save uses the preset name, not the chain name (#518).
- **Confirm before removing a project** (#25).
- **Chain reorder** via ▲/▼ keeps the selection on the moved row and
  persists across save/reload (#502).
- **Project picker** + per-input bank/scene navigator in the GUI (#453).
- **`--project` CLI flag + `OPENRIG_PROJECT` env var** for opening a
  specific project file at launch (#452).

### Performance
- **Block enable/disable is instant.** `Command::ToggleBlockEnabled` now
  flips the existing `BlockRuntimeNode.fade_state` (the click-safe
  `FadingOut` / `FadingIn` transitions the engine already supported) on
  every per-input runtime of the chain — no `upsert_chain`, no CPAL
  queries, no NAM reload, no graph rebuild (#522).
- **Chain enable/disable is instant.** Disable now flips
  `set_draining()` on the live runtime instead of dropping it; re-enable
  clears the flag in O(1). The CPAL streams + every block processor
  stay alive, so toggling between chains during a session no longer
  rebuilds anything (#522).

### Audio I/O
- **Multi-interface support per chain** — one chain can use input
  entries from multiple physical devices (#17).

### GUI
- **Per-chain input and output level meters** in the chain row, pre-FX
  and post-FX, post-volume (#32, #36).
- **Multi-language support (i18n)** — 9 locales (de, en, es, fr, hi, ja,
  ko, pt-BR, zh) with locale-aware default font (#9).
- **NAM logo asset** integrated into the block library (#35).

### MIDI
- **MIDI/OSC adapter (incl. BLE-MIDI)** for footswitch + controller
  integration — bind any controller event to a `Command` variant via the
  MIDI binding file (#22).

### MCP server
- **OpenRig is now an AI-controllable tool via the Model Context
  Protocol.** Every `Command` variant becomes a tool automatically; the
  live project, devices, chain/block IDs and per-chain meters are
  exposed as resources. Drive the running rig from any MCP client
  (#165).

### Plugins
- **Cross-platform plugin compatibility** for the bundled plugin tree —
  packages now ship binaries for every supported target (#5).

### Build / Distribution
- **Shared `xgodev/quality-gate`** replaces the project-local CI gate —
  comparative against `origin/develop`, tamper-resistant (#482).
- **Release workflow now builds `qa_audit` and pulls Git LFS objects**
  inside cargo's git checkout, so the linker sees the real
  `libNeuralAudioCAPI.so` instead of a 1-line LFS pointer (#525).

## v0.1.0-dev.22 — 2026-05-17

Linux production polish (#479 umbrella) — the four remaining bugs that
made the Linux build unusable as a real product.

- **Block-editor window resizes correctly** on Linux WMs that ignore
  Slint min/max/preferred — explicit `window().set_size()` from Rust.
- **Presets appear in the desktop picker.** Replaced the native
  `FileDialog` branch with the in-app overlay used by kiosk mode.
- **Find / search works in compact view.** The model picker now mutates
  the existing `VecModel` in place instead of swapping `ModelRc`.
- **AppImage builds inside Docker emulation.** Assembled manually
  (type-2 runtime + `mksquashfs` + `cat`) instead of executing
  `appimagetool`, which crashed with `Exec format error` (#479, #481).

## v0.1.0-dev.21 — 2026-05-17

Plugin loader + Linux/macOS packaging hardening.

- **LV2 validation tolerates platform-specific binaries.** Packages no
  longer have to ship a binary for every supported target; the validator
  only requires the host's binary (#477).
- **Linux installers (.deb / .rpm / .tar.gz) ship a `.desktop` file and
  icon** so OpenRig shows up in Applications menus (#475).
- **README documents the macOS quarantine step** so first-launch no
  longer dead-ends users with a "damaged" error (#471).
- **Linux audio prerequisites documented** (jackd2, `audio` group, USB
  interface) so the install path stops failing silently (#473).
- **`patchelf` added to the Linux builder Dockerfile** — local Linux
  build was breaking without it (#468).

## v0.1.0-dev.20 — 2026-05-16

Packaging hardening across all three platforms.

- **macOS Developer ID codesign + notarization.** Tarballs no longer
  open as "damaged" (#459).
- **macOS build no longer dies with `printf: Broken pipe`** — regression
  of #459 caught and resolved (#463).
- **Linux `.deb` declares `libseat` and ships `libNeuralAudioCAPI.so`**
  inside the bundle so the app actually starts (#461).
- **Linux builder Dockerfile no longer hardcodes `cortex-a53`** as the
  target CPU on every architecture — local x86_64 builds work again
  (#466).
