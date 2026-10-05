# Quality Gate — OpenRig

OpenRig uses the **shared** quality gate from [xgodev/quality-gate](https://github.com/xgodev/quality-gate), the same for every project. It is **comparative**: it fails only when the PR makes a metric **worse** than the PR's base branch; pre-existing debt never blocks.

**It runs only in the PR's CI** — the `quality-gate` job of `.github/workflows/pr.yml`, on every PR into `develop`, a `release/**` branch or `main`. Never run it on a developer's machine: in Rust it compiles the workspace twice (base + PR).

## How the CI job runs it

- The gate ships as the Docker image `ghcr.io/xgodev/quality-gate/rust:v1` (Rust toolchain + the gate). The job installs OpenRig's native build deps (ALSA, JACK, fontconfig, …) inside the container, then runs the gate's entrypoint `/opt/quality-gate/qg --base origin/<base branch>`.
- The baseline is the PR's **own base branch** (`github.base_ref`), never a fixed `develop`.
- The job blocks only on the JSON `.verdict == "regressed"`. `passed`, `improved`, `same` and `bypassed` pass; a missing verdict (a tool or setup error) does not block, because the separate `Test Suite` job builds and tests for real.
- On failure: a sticky comment (header `openrig-quality-gate`) with the regressed metrics + a formal request-changes from `github-actions[bot]`. On success: a ✅ comment and the request-changes is dismissed. The logs are uploaded as the `qg-logs` artifact.

## Philosophy

The gate fails when the PR makes the project **worse**. However much pre-existing debt there is, a PR that does not add to it passes. Every PR may reduce debt; none may increase it.

## Compared metrics (PR vs base) — Rust

| Metric | How it counts |
|---|---|
| `fmt` | `cargo fmt --check` (the gate's embedded rulesets) |
| `lint` | `cargo clippy -D warnings`, without complexity |
| `build` | `cargo build --all-targets` |
| `test` | `cargo test --no-fail-fast` (number of failures) |
| `complexity` | clippy cognitive / lines / args / type (the gate's defaults) |
| `coverage` | `cargo llvm-cov` → `lines.percent`, margin `QG_COV_MARGIN` |

The gate **ignores** the project's `clippy.toml`/`rustfmt.toml` on purpose (tamper resistance) and uses its own rulesets (community defaults: cognitive 25, lines 100, args 7, type 250). Our `clippy.toml` applies only to a local `cargo clippy` and to `scripts/validate.sh`.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Passed / bypassed / no relevant supported language |
| 1 | At least one metric regressed vs the base |
| 2 | Tool or setup error (NOT a regression — read the stderr) |
| 3 | No supported language detected |

## Forbidden

Silencing the gate without a real fix:

- Setting `QG_BYPASS_REASON` on your own (it forces exit 0 and writes an audit log).
- Raising thresholds in `clippy.toml` (useless — the gate ignores it) or editing code, tests or config just to "pass".
- Marking tests `#[ignore]`.
- `#[allow(clippy::...)]` without a justified root cause.
- `--no-verify` on a commit.

The rule: **root cause, or escalate**.

## Before the push: `validate.sh` and `cargo fmt`

`scripts/validate.sh` is **not** the gate. It runs OpenRig's own static rules that the generic gate does not cover: the responsibility header, the LOC caps, the minimum font size and the ban on inline `#[cfg(test)] mod tests`. Before every push, over the WHOLE repo:

```bash
cargo fmt --all -- --check
VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
```

The same whole-repo check runs in CI as the `Static Checks (whole repo)` job.
