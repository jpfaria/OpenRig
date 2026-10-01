# OpenRig — Claude Code

Virtual guitar rig/pedalboard in Rust + Slint. macOS, Windows, Linux.

## Laws

1. **The main folder is the owner's.** The agent never touches it: no `git`, `Edit`/`Write`, stage, worktree, or file created via Bash (`touch .dev-rules/…`, `mkdir`, temp files) — not even when the owner says "turn the gate off". Work only in `.solvers/issue-N/`, a real clone made by `scripts/solver-setup.sh <N> <branch> [release-base]`. `git worktree` is forbidden. If something must happen in the main folder, hand the owner the command. Details: `docs/development/gitflow.md` → "Workspace isolado".
2. **Short chat, in pt-BR.** 1–2 sentences; no tables, headers, recaps or "next steps". A question asks one thing in one line. Long diagnosis goes to the issue. Repo content (code, comments, docs, commits, branches, PRs, issue comments) is English; only `README.pt-BR.md` / `README.es-ES.md` are not.
3. **Validation checklist** whenever the owner must validate (ear, eye, hardware, real app), in chat AND on the issue: the main-folder command `git fetch && git checkout {type}/issue-N && git pull`, the absolute `run:` line printed by `scripts/solver-setup.sh`, then `1. [ ]` items of what only he validates. The only list allowed in chat. Format: `docs/development/gitflow.md` → "Checklist de validação".
4. **Never assume; never ask the obvious.** Scope, data model, behaviour, layer, file or A-vs-B unclear → stop and ask one short question at a time, never pick an alternative he did not pick. Already agreed or inferable from code/context → decide and go.
5. **Issue content lives in the issue.** History, diagnosis, hypotheses, measurements, decisions, "why" stories and status go to `gh issue comment`. The repo states only what is true now: no `#N` in `CLAUDE.md`, skills, hooks, `docs/**` or code comments. Found one in a file you touch → post it on its issue, leave the current truth.
6. **N streams = N isolated pipelines.** Own input, runtime, output, rebuild, failure, latency and CPU scheduling. Runtimes on the I/O path are selected by stream/device identity — never by rate, never "all that match". Mixing happens only in the backend (cpal/JACK). Code where two streams know each other is a bug: stop, report, fix. Details: `docs/audio-config.md` → "Isolation is by stream identity".
7. **One file, one responsibility.** Every production file opens with `//! Responsibility: <one sentence>` (`// …` in `.slint`) with no "and"/list. A new responsibility is a new file, never the end of an open one. Caps `.rs` 600 / `.slint` 500 are only the alarm; tests have no cap; `mod.rs`/`lib.rs` < 100 LOC. Details: `docs/development/file-organization.md`.

## Real-time invariants — never regress

1. Round-trip latency. 2. Audio quality (noise, aliasing, THD, frequency response). 3. Stream stability — zero xruns, dropouts, clicks. 4. Stream isolation (law 6). 5. Streams are stereo inside: mono → `Stereo([s,s])`, DualMono → independent `[L,R]`; mono out only via `OutputBlock.mode == mono` (`apply_mixdown`); never auto-pan. 6. Stable callback jitter. 7. Audio-thread CPU. 8. Zero allocation, lock, syscall or I/O on the audio thread. 9. Golden samples within tolerance. 10. Per-stream volume is immutable unless the owner asks; if `crates/engine/src/volume_invariants_tests.rs` breaks, the source is wrong, not the test.

**Red flags — stop and report:** new xrun/click, latency +1 ms unexplained, golden tests failing, `Mutex`/log/I/O in processing, "the sound changed on one OS", "the volume changed", any state shared by 2+ streams.

**Trade-offs:** sound + stability + isolation > latency > audio-thread CPU > cross-platform > ergonomics > new features. A feature never justifies a regression; discuss first.

**Cross-platform:** never hardcode paths; Linux/Orange Pi/JACK fixes stay behind `cfg`.

## Code

- Zero warnings. Blocks never name specific models/brands. Constants once. No visual config in business logic.
- Every state change is a `Command` (`crates/application/src/command.rs`); the GUI calls `dispatcher.dispatch`, MCP/gRPC share the variant; never `borrow_mut()` in a callback. Slint is a pure dispatcher (callback → `Event` → pure fn); no `AppWindow` in tests. Core (`State`/`Event`/`Command`/`SideEffect`) has no Slint dep.
- System `config.yaml` vs `project.yaml`: "must this value travel with the project file?" Yes → project. `docs/adr/0003-system-vs-project-config.md`.
- Docs are part of the task, same commit. READMEs always in the 3 languages.
- UI work: `docs/development/ui-rules.md` first.

## Tests and delivery

- TDD red-first: no production change without a test that failed first. Two cargo rounds per delivery (all RED, then all GREEN), targeted tests only; one commit + push at the end. Full suite and quality gate run only in CI. Before push: `cargo fmt --all -- --check` + `VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates`. `docs/testing.md`.
- Audio problem: search the issues first (`gh issue list --repo jpfaria/OpenRig --state all --search '<symptom>'`); record numbers, repro test and each hypothesis (CONFIRMED/REFUTED/OPEN) on the issue; never retest a REFUTED one without new evidence; done only when the issue names a green test reproducing the measured symptom. Hardware battery: `OPENRIG_HW_TESTS=1`.
- Gitflow: `docs/development/gitflow.md`. Branch `{type}/issue-N` from the active `release/vX.Y.Z` (highest version with no tag); work ends at the push; PR/merge only on request; `gh issue comment` after every push.

## Posture

Only what was asked — no unrequested crate, binary, issue, PR or refactor. Invoke the relevant skill before non-trivial work. Map scope and root cause before touching code. Never revert a commit or delete a file the agent made (redo on top); delete only the literal scope asked; no regex/sed content migrations.

## References

`docs/architecture.md` (crates, registry) · `docs/audio-config.md` (I/O, JACK) · `docs/blocks-catalog.md` · `docs/screens.md` · `docs/cli.md` · `docs/scripts.md` · `docs/testing.md` · `docs/development/` (gitflow, file-organization, ui-rules, release) · `docs/hardware/orange-pi-deploy.md` · `CONTRIBUTING.md`
