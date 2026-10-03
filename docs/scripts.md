# Build and deploy scripts

| Script | What it does |
|--------|--------|
| `scripts/solver-setup.sh <N> <branch> [release-base]` | Builds the agent workspace `.solvers/issue-N` (clone, `plugins` link) and prints the absolute run command with `OPENRIG_PLUGINS_ROOT` |
| `scripts/validate.sh [files…\|crates]` | OpenRig's static rules (responsibility header, LOC caps, minimum font size, no inline test modules); `VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates` is the pre-push check over the whole repo — see [quality gate](development/quality-gate.md) |
| `scripts/pre-pr-gate.sh` | Runs CI's checks locally (fmt, whole-repo static checks, the workspace tests through cargo-nextest when installed) on the committed HEAD and stamps it; required before `gh pr create` and before pushing to a branch with an open PR — the `.claude/hooks/pre-pr-gate-guard.sh` hook denies both otherwise. Tested by `scripts/test_pre_pr_gate.py` |
| `scripts/nam_vendor.py check\|update\|push` | The vendored NeuralAmpModelerCore archive (`deps/NeuralAmpModelerCore.tar.gz` + `.lock`): `check` names a newer upstream release (the first local build after a `git fetch` runs it), `update` vendors it and commits on the current branch, `push` publishes that commit — CI's `nam-refresh` job does both on `develop` after the tests pass. Offline / timeout / failed commit / refused push = warning, exit 0 |
| `scripts/install-macos.sh` | The one-line `curl` installer: downloads the release `.dmg`, copies the app to `/Applications`, strips quarantine |
| `scripts/install-macos-local.sh [version]` | Dev: builds the current checkout (through `package-macos.sh`) and installs it in `/Applications` (quits a running instance, then opens it). `OPENRIG_PLUGINS_DIR` is passed to the packager. `version` defaults to `dev` |
| `scripts/package-macos.sh [version]` | Packages macOS: an ad-hoc inside-out signature + a verification gate, then `dist/OpenRig-<version>-macos-universal.dmg`. Bundled plugins: `OPENRIG_PLUGINS_DIR=/path/plugins/source` overrides the source (default `plugins/source`; an override that does not exist is a fatal error) |
| `scripts/package-linux.sh` | Packages Linux `.tar.gz`/`.deb`/`.rpm`/`.AppImage` (patchelf RUNPATH for `libnam_wrapper` + `libseat`) |
| `scripts/package-windows.ps1` | Packages Windows: a `.zip` bundle and a WiX `.msi` installer, from already-built release binaries |
| `scripts/lib/console-binaries.{sh,tsv}` | The single source of the console-style binaries shipped next to the GUI (`openrig-console`, `openrig-console-rig`, `openrig-render`). The packagers build and stage from it; tested by `scripts/tests/console_bundle_test.sh` |
| `scripts/lib/plugins-bundle.sh` | Plugin-bundling helpers sourced by the packagers; tested by `scripts/tests/plugins_bundle_test.sh` |
| `scripts/lib/release-version.sh` | `release_version_from_tag` (ref → semver) + `set_workspace_version` (writes `[workspace.package]`'s `version` without touching the `[workspace.dependencies]` pins). The tag is the source of truth: every `release.yml` job runs it BEFORE compiling, because the footer renders `env!("CARGO_PKG_VERSION")`. Refuses non-semver input; tested by `scripts/tests/release_version_test.sh` |
| `scripts/build-deb-local.sh` | Cross-compiles the arm64 + amd64 `.deb` through Docker into `output/deb/` |
| `scripts/build-linux-local.sh` | The Linux build (internal, called by `build-deb-local.sh`) |
| `scripts/deploy-orange-pi.sh --host USER@IP` | Builds the arm64 `.deb` and installs it on an Orange Pi over SSH |
| `scripts/build-orange-pi-image.sh` | An SD card image for the Orange Pi |
| `scripts/flash-sd.sh` | Flashes the SD card |
| `scripts/build-lib.sh` | Builds the external LV2 plugin libraries — see [Building](development/building.md#lv2-plugin-libraries) |
| `scripts/add-dep.sh <name> <repo-url> [commit]` | Adds a git submodule dependency pinned to a commit |
| `scripts/coverage.sh` | HTML coverage report |
| `scripts/patch-coverage.sh [base] [--files] [--fresh]` | PATCH coverage — the lines the branch adds, the same number `codecov/patch` reports on the PR. `PATCH_COV_OFF=1` turns it off |
| `scripts/extract-translations.sh` | Refreshes `translations/adapter-gui.pot` from the `@tr(...)` strings in the `.slint` files and merges it into each locale's `.po` |
| `scripts/po_reconcile.py` | Recovers human translations across key or context renames in a `.po`, so a renamed key does not leak to the UI untranslated |
| `scripts/gui-command-inventory.py` | Inventory of every GUI callback: whether it goes through a `Command`, changes state without one, or is screen-only |
| `scripts/gui-command-coupling.py` | Counts GUI callbacks that change state without dispatching a `Command`; the number only goes down, the goal is 0 |
| `scripts/release-downloads.sh` | Download counts of the GitHub releases |

## macOS dmg — naming and installing

A local macOS build ships as `OpenRig-<version>-macos-universal.dmg`: the version belongs in the file name, because these get installed side by side while validating a release and an unversioned file says nothing about what is in `/Applications`.

```bash
OPENRIG_PLUGINS_DIR=<plugins> ./scripts/package-macos.sh <version>
```

**The packager does not change the compiled version.** The app renders the compiled-in `CARGO_PKG_VERSION`, and the release workflow sets it only from the tag, so a pre-tag build shows the previous version on screen. For a dmg that stands for a release, set it first with `set_workspace_version` (`scripts/lib/release-version.sh`, e.g. `0.4.2-921`) and revert the manifest afterwards. The packager already signs ad-hoc inside-out and verifies (`codesign --sign -`); the install still strips quarantine.

**"Build me a DMG to install" means build it AND install it**: build in the solver with `OPENRIG_PLUGINS_DIR` and a local version, then quit the app, mount, `rm -rf /Applications/OpenRig.app`, copy, detach, strip quarantine, `open -a OpenRig`, and report the installed version — never hand back a command to paste when the agent can run it. This is **not** a licence to replace `/Applications` when no install was asked for.

## Branch → .deb → Orange Pi

```bash
git checkout {type}/issue-{N} && git merge origin/release/vX.Y.Z
./scripts/build-deb-local.sh
scp output/deb/openrig_<version>_arm64.deb root@<board-ip>:/tmp/
ssh root@<board-ip> "dpkg -i /tmp/openrig_<version>_arm64.deb && systemctl restart openrig.service"
```

`scripts/deploy-orange-pi.sh --host root@<board-ip>` does the same in one step (the SSH password goes in `--password` or `SSHPASS`).

## Build rules

- Never compile on the board. Always cross-compile on the Mac with `build-deb-local.sh`.
- Docker Desktop must be running (the build uses an arm64 container).
- Only arm64 goes to the Orange Pi (amd64 is for x86 Linux).

## `cargo clean` in `.solvers/`

Workspaces in `.solvers/issue-N/` pile up inconsistent state in `target/` across merges, edits in several crates, branch switches or sharing with Docker. Symptoms: `error[E0460]: possibly newer version of crate X`, `error[E0463]: can't find crate`, an ICE in `rmeta/decoder.rs`, a green build but a runtime "fn X not found".

Before ANY build the owner will consume:

```bash
cd .solvers/issue-N && cargo clean && ./scripts/build-deb-local.sh
```

Mandatory after: a `git merge`, editing a struct/enum in 2+ crates, changing a `#[cfg(...)]`, the first `build-*local.sh` of the session.
