# Gitflow — OpenRig

```
Issue → Branch (from the active release/vX.Y.Z) → Commits → PR → Review/Merge
```

**Flow:** `feature/bug → release/vX.Y.Z → main → develop` · `hotfix → main → develop`

| Branch | Purpose | Merges into |
|---|---|---|
| `main` | Production — the `vX.Y.Z` tag triggers the **final release**. Never behind what shipped. | `develop` (back-merge) |
| `develop` | The reference, **always ahead**. `release/vX.Y.Z` is cut from it. | — |
| `release/vX.Y.Z` | The version's cycle: receives features and fixes; a `vX.Y.Z-beta.N` tag triggers a **beta**. | `main` |
| `feature/*` | Features | the active `release/vX.Y.Z` |
| `bug/*` | Fixes | the active `release/vX.Y.Z` |
| `docs/*` | Documentation only | the active `release/vX.Y.Z` |
| `hotfix/*` | Production emergencies | `main` (+ back-merge into `develop`) |

**Cycle:** (1) cut `release/vX.Y.Z` from `develop`; (2) features and fixes enter it by PR; (3) a `vX.Y.Z-beta.N` tag on the release → a **pre-release** build; (4) PR `release/vX.Y.Z → main` + a `vX.Y.Z` tag on `main` → the **final release** (the milestone closes, the bump goes to `develop`); (5) back-merge `main → develop`.

## Rules

1. **Issue first.** `gh issue list --search` before creating one (avoid duplicates). Never create an issue without the owner asking.
2. **Branch name: `{type}/issue-{N}`** (`feature`, `bug`, `docs`, `hotfix`) — no descriptive suffix. Before creating it: `git fetch && git branch -a | grep issue-{N}`.
3. **From the active, up-to-date `release/vX.Y.Z`**: `git fetch && git checkout release/vX.Y.Z && git pull`. No active release yet? Cut it from `develop`: `git checkout develop && git pull && git checkout -b release/vX.Y.Z && git push -u origin release/vX.Y.Z`. **The version follows what the release carries:** a bug opens a PATCH (`vX.Y.Z+1`), a feature opens a MINOR (`vX.Y+1.0`) — never open `vX.Y+1.0` to fix a bug. See `release.md` §8.

   **Active = the release NOT finalized yet.** A finalized release has its `vX.Y.Z` tag and is merged into `main` — working on it (or opening a PR to it) ships code that never goes out, because that version is already published. The `release/vX.Y.Z` branch existing means nothing: old ones stay on the remote. `develop` being at version X.Y.Z means nothing either — the bump happens when the release is cut. **Mandatory check before cutting a branch and before `gh pr create`:**

   ```bash
   git fetch --tags
   git branch -r | grep release/          # candidates
   git tag -l 'v0.4.0'                    # empty = not finalized
   git log --oneline -1 origin/main       # "Merge release/vX.Y.Z into main" = that one is done
   ```

   The active one is the HIGHEST version with no tag.

   **Exception — a change touching only `site/`: straight to `main`.** Pages publishes from `main` (`.github/workflows/pages.yml`, `paths: site/**`), so going through the release and `develop` only delays publishing. Clone `main`, commit and push to `main` — no issue branch, no PR. Valid only while the diff is exclusively `site/` (plus the docs of the rule itself).

   **Exception — a docs-only change (`*.md`, `docs/**`, `.claude/skills/**`, `CLAUDE.md`): no PR.** Merge the issue branch straight into the active release, push, then release → `develop` (direct merge, push).

4. **Merge the active release before any work**: `git merge -X theirs origin/release/vX.Y.Z`.
   A clean merge can still break the build: when both sides added the same struct field, git keeps both lines (`E0062`). `./scripts/pre-pr-gate.sh` catches it; run it before `gh pr create` and before pushing to a branch with an open PR.
5. Commits in English, no `Co-Authored-By`, focused on the "why".
6. **Never `Closes #N` or `Fixes #N`** in commits — GitHub auto-closes.
7. A bug or hotfix merges right away. A feature waits for review. Never merge `feature → release` without the owner asking.
8. **Never rebase.** Always `git merge`, never `git pull --rebase`.
9. **The quality gate runs only in the PR's CI — never locally, never per push.** The **shared** gate `xgodev/claude-plugin` runs in the PR's CI (`.github/workflows/pr.yml`): a failure there = a sticky comment + an automatic request-changes. In Rust it compiles the workspace twice (base + branch); running it on the developer's machine is forbidden. Details in [`quality-gate.md`](quality-gate.md).
10. **One commit and one push per delivery, not per step.** Every push triggers the CI build: 800 commits = 800 compiles. Push right after that commit.
11. **PRs are always non-interactive**, with the right `--base` for the flow: `feature/bug → ` the active `release/vX.Y.Z`; `release/vX.Y.Z → main`; `hotfix → main`; back-merge `main → develop`. Push the branch first, then `gh pr create --repo jpfaria/OpenRig --base <target> --head <branch> --title "…" --body "…"` — every field explicit. Without `--title`/`--body`/`--head` (or with the branch not pushed) gh opens its interactive prompt and **hangs** in a shell with no TTY until the timeout (~8 min). Guard-rail: `gh config set prompt disabled` (gh errors at once instead of hanging).
12. **Stage explicit paths** — never `git add -A` in `.solvers`.
13. **Before planning:** check `docs/superpowers/specs/` + `gh issue list`. Do not multiply issues (each one becomes a branch and a workspace of several GB). `@claude` on GitHub follows the mandatory-premises template.

## Closing an issue

Only when the owner asks. Before closing, set the milestone — **plain semver**:

1. The milestone is the **version of the active `release/vX.Y.Z`** — the open `vX.Y.Z` milestone. `gh api repos/jpfaria/OpenRig/milestones --jq '.[].title'` lists the open ones.
2. **Never create or reopen a `vX.Y.Z-dev.N` milestone** (a dead scheme) nor `-beta.N` (a beta is a tag, not a milestone). Use the open `vX.Y.Z` milestone.
3. `gh issue edit <N> --milestone "vX.Y.Z"` → `gh issue close <N>`.

**Merging the PR does NOT close the issue.** A `Closes #N` in the PR body only fires when the base is the repo's default branch — and here every feature/bug PR targets the active `release/vX.Y.Z`. After the merge the issue stays OPEN and closing it is manual, milestone first.

## Labels that exclude from the release notes

- `duplicate` — the same scope as another issue (the duplicate is the newer one).
- `internal` — CI/CD, scripts, workflows, build deps, configs, planning, changes the end user does not see.

## Isolated workspace (.solvers/)

Never edit code in the main folder. Each agent works in a **clone** (`.solvers/issue-N`).

**`git worktree` is FORBIDDEN** — any kind, anywhere. A worktree shares the main folder's `.git` and locks the branch, so the owner's `git checkout` in his folder aborts. Isolation is always a clone with its own `.git` in `.solvers/issue-N`. Before working, check that `.solvers/issue-N/.git` is a DIRECTORY (a `.git` file = a worktree).

**Control files live where the work lives.** `.dev-rules/.off`, `.dev-rules/.mode-feature`, `.dev-rules/.red-first-unlocked`, `mkdir`, temp files, gate markers — none of them is created in the main folder, not even when the owner says "turn the gate off". `main-folder-guard.sh` blocks only `Edit`/`Write`/`git`, so a Bash `touch` passes: create the file inside `.solvers/issue-N/`, or hand the owner the command (`! touch .dev-rules/.off`) or ask for a relaunch with `DEV_RULES_OFF=1`.

**Two folders, symmetric isolation:**

| Folder | Who uses it | Who does NOT |
|---|---|---|
| main (`/Users/<user>/.../OpenRig`) | the owner — editing, visual checks, tests on real hardware | the agent — never `git checkout`, `pull`, `commit`, `push`, edit, revert |
| `.solvers/issue-N/` | the agent — implementation, commits, push, targeted `cargo test` | the owner — he does not work from here |

To test an agent's branch, the owner runs `git fetch && git checkout {type}/issue-N && git pull` **in his main folder**, or runs the app straight from the solver with the `run:` line below.

**Setting up the workspace = `scripts/solver-setup.sh <N> <branch> [release-base]`.** It clones (never a worktree), creating and pushing the branch from `release-base` when it is not on the remote yet; brings in the NAM sources; links `plugins` to the config's `paths.plugins_path` (kept out of `git status` through `.git/info/exclude`); and prints the absolute `run:` command for the checklist. Re-running it on an existing workspace only completes what is missing, and it fails loudly when it cannot find the plugins.

**Where the workspace lives is the machine owner's call.** When his global `CLAUDE.md` names a workspaces folder (e.g. an external disk, to keep clones and their `target/` off the internal disk), run `OPENRIG_SOLVERS_ROOT=<that folder> scripts/solver-setup.sh …`: the clone goes to `<that folder>/issue-N` and `.solvers/issue-N` becomes a link to it, so every path, guard and `run:` line stays `.solvers/issue-N`. The script fails loudly when the folder does not exist (disk not mounted). Without `OPENRIG_SOLVERS_ROOT` the clone lives in `.solvers/issue-N` itself.

Without the `plugins` link an app opened from the solver loads **zero** plugin packages — any check involving NAM/IR/LV2/VST3 says "not found" and looks like a code bug. Check the app's startup log: `plugin catalog ready: … (N native, M disk package(s))` with `M > 0`.

After the merge, delivery ends only with these three steps — none is automatic:

1. **Close the issue** with the milestone set first (see [Closing an issue](#closing-an-issue)) — a merge into `release/vX.Y.Z` closes nothing by itself.
2. **Delete the branch, remote AND local.** GitHub's auto-delete-on-merge covers the remote only when it is on; the local one in `.solvers/issue-{N}` never goes away by itself. `git push origin --delete {type}/issue-{N}` (or `gh api -X DELETE repos/jpfaria/OpenRig/git/refs/heads/{type}/issue-{N}`), and check with `gh api repos/jpfaria/OpenRig/git/refs/heads --jq '.[].ref'` — a leftover work branch is permanent clutter on the remote.
3. **Remove the workspace:** `rm -rf .solvers/issue-{N}/` — only once the issue is CLOSED, because `rm -rf` takes any uncommitted WIP with it. Confirm with `gh issue view N --json state`; the workspace of an OPEN issue is off-limits even on a generic request ("clean the solver", "clean up the junk").

## Sibling issues

How to recognise one: its **body** starts with `> **Sibling issues (co-evolving in this cycle):** #<other>`. Before any work on a sibling issue: `git fetch && git merge origin/{type}/issue-<sibling> --no-edit`. Sync on every logical commit.

## Traceability — comments on the issue

The issue is the audit log. Comment with: the plan before starting; each push (hash + files + build/test); a change of plan; each problem with its evidence; technical analysis; merges; hardware validation; the final summary. After a `git push` or a technical analysis, the next command is `gh issue comment <N>`. Options A/B/C for the owner go on the issue BEFORE the question.

### Validation checklist

**Mandatory in every delivery the owner must validate** (ear, eye, hardware, behaviour in the real app): the issue comment AND the chat reply carry a checklist with:

1. TWO commands, each in its own code block, always both: the main folder `git fetch && git checkout {type}/issue-N && git pull`; and the solver = the `run:` line printed by `scripts/solver-setup.sh <N> <branch>`, verbatim (absolute path + `OPENRIG_PLUGINS_ROOT=<plugins folder>`, e.g. `cd /Users/…/OpenRig/.solvers/issue-N && OPENRIG_PLUGINS_ROOT=/Users/…/OpenRig-plugins/plugins/source cargo run -p adapter-gui -- --mcp`; without `OPENRIG_PLUGINS_ROOT` the app opens with no plugins).
2. NUMBERED checkbox items (`1. [ ]`, `2. [ ]`, …), one per line, only what HE validates — never the tests or builds the agent already ran.

No prose around it. It is the only list allowed in chat (`CLAUDE.md`, laws 2 and 3).

## Release mechanics

Step-by-step for actually cutting one: [`release.md`](release.md).

- **Two tag kinds, both trigger `release.yml`** (`on: push: tags: v*`), which derives the version from `GITHUB_REF_NAME`:
  - **Beta:** tag `vX.Y.Z-beta.N` on the **`release/vX.Y.Z`** branch → a GitHub **pre-release** (auto-generated notes; the milestone stays OPEN; no version bump to develop).
  - **Final:** tag `vX.Y.Z` on **`main`** → a full release (curated milestone notes; milestone closes; the bump is pushed to `develop`). Ship it via the `release/vX.Y.Z → main` PR, then tag `main`; afterwards back-merge `main → develop`.
  - A tag is a pre-release iff its name contains a `-` (semver pre-release), so `create-release` adds `--prerelease` and `commit-version-bump` is skipped for those.
  - Re-trigger a failed release by deleting and recreating the tag ref at the new tip of its branch.
- **The tag is the source of truth for the version — never bump `Cargo.toml` by hand.** Every build job runs `scripts/lib/release-version.sh` to write the tag's version (including a `-beta.N` pre-release) into `[workspace.package]` *before* compiling, because the launcher footer renders `env!("CARGO_PKG_VERSION")`; skipping that ships artifacts whose binary reports the previous version. After a **final** release, the `commit-version-bump` job re-applies the same bump plus `cargo update --workspace` on **`develop`** (the always-ahead reference and manifest source of truth) and pushes it, so the repository never drifts behind the last tag; `main` never falls behind because every release flows `release/vX.Y.Z → main → develop`. The helper is covered by `scripts/tests/release_version_test.sh` and refuses any non-semver input rather than writing an unparseable manifest.
- **A release ships macOS only.** The Linux x86_64, Linux aarch64 and Windows x64 jobs carry a hard `if: false`, so `release.yml` produces a single artifact and the job list shows three skipped builds — expected, not a failure. Packaging is exercised **only** at release-tag time (PR CI never builds installers), so regressions surface one ~25-min failure at a time after the tag. Re-enabling a platform means flipping its build job **and** the matching artifact download in `create-release`.
- The loudness audit (`qa_audit`, ~22 min) does NOT run in the release path (`QA_AUDIT_SKIP=1`) — it belongs to OpenRig-plugins CI. Keep it that way.
