# Contributing to OpenRig

## Git Flow

This project follows [Gitflow](https://nvie.com/posts/a-successful-git-branching-model/).

### Branches

| Branch | Purpose | Merges into |
|--------|---------|-------------|
| `main` | Production-ready releases | — |
| `develop` | Integration branch for next release | `main` |
| `feature/*` | New features | `develop` |
| `bugfix/*` | Bug fixes during development | `develop` |
| `hotfix/*` | Urgent fixes for production | `main` + `develop` |
| `release/*` | Release preparation | `main` + `develop` |

### Branch naming

```
feature/issue-{N}-short-description
bugfix/issue-{N}-short-description
hotfix/issue-{N}-short-description
release/v{X.Y.Z}
```

## Development Workflow

Every change follows this flow:

```
Issue → Branch → Commits → PR → Review/Merge
```

### 1. Create an Issue

Every bug fix or feature must have a GitHub issue before any code is written.

```bash
gh issue create --title "Short description" --label "bug"       # for bugs
gh issue create --title "Short description" --label "feature"    # for features
gh issue create --title "Short description" --label "enhancement" # for improvements
```

### 2. Create a Branch

One branch per issue, from `develop`:

```bash
git checkout develop
git pull origin develop
git checkout -b feature/issue-{N}-short-description   # for features
git checkout -b bugfix/issue-{N}-short-description     # for bugs
git checkout -b hotfix/issue-{N}-short-description     # for urgent production fixes (from main)
```

### 3. Commit

- Commit messages in **English**
- Focus on the "why", not the "what"
- Reference the issue: `Closes #N`
- No `Co-Authored-By` lines

### 4. Push and Create a PR

```bash
git push -u origin feature/issue-{N}-short-description
gh pr create --title "Short title" --body "Closes #N" --base develop
```

### 5. Merge Policy

| Type | PR base | Action |
|------|---------|--------|
| **Bugfix** | `develop` | Merge immediately after PR is created |
| **Hotfix** | `main` | Merge immediately, then merge `main` back into `develop` |
| **Feature** | `develop` | PR stays open for review before merging |
| **Enhancement** | `develop` | PR stays open for review before merging |

### 6. Releases

```bash
git checkout develop
git checkout -b release/v1.0.0
# bump version, final testing
gh pr create --title "Release v1.0.0" --base main
# after merge into main, tag and merge back into develop
git tag -a v1.0.0 -m "Release v1.0.0"
```

## Parallel Agents with Git Worktrees

Multiple AI agents (Claude Code sessions) can work on different issues simultaneously using Git worktrees. Each agent gets an isolated copy of the repo with its own branch.

### Directory structure

```
OpenRig/                              ← develop (main workspace)
OpenRig-worktrees/
  issue-4-lv2/                        ← feature/issue-4-lv2-plugin-host
  issue-1-48khz/                      ← feature/issue-1-force-48khz
  issue-3-timbre/                     ← bugfix/issue-3-timbre
```

### Creating a worktree for an issue

```bash
# From the main repo
cd /path/to/OpenRig

# Create the branch from develop
git checkout develop
git pull origin develop
git checkout -b feature/issue-{N}-short-description

# Create the worktree
git worktree add ../OpenRig-worktrees/issue-{N}-short-description feature/issue-{N}-short-description
```

### Agent naming convention

Each agent has a short name matching the issue topic:

| Agent Name | Worktree | Branch | Issue |
|------------|----------|--------|-------|
| `lv2` | `issue-4-lv2/` | `feature/issue-4-lv2-plugin-host` | #4 |
| `48khz` | `issue-1-48khz/` | `feature/issue-1-force-48khz` | #1 |
| `timbre` | `issue-3-timbre/` | `bugfix/issue-3-timbre` | #3 |

### Rules

- **One agent per worktree** — never share a worktree between agents
- **One branch per issue** — never mix issues in a single branch
- **Always start from develop** — create the branch from latest develop
- **Merge develop regularly** — keep your branch up to date with `git merge develop`
- **PR when done** — push and create PR to develop, then remove the worktree

### Cleanup

```bash
# After PR is merged
git worktree remove ../OpenRig-worktrees/issue-{N}-short-description
git branch -d feature/issue-{N}-short-description
```

### Listing active worktrees

```bash
git worktree list
```

## Product Priorities (Non-Regression)

OpenRig is a real-time audio processor. **Sound quality and latency are the core product values.** No feature, fix, refactor, or dependency bump may be merged unless it proves it does not degrade any of the properties below. These priorities override code ergonomics, delivery speed, and even new features.

### Invariants that MUST NOT regress

1. **Round-trip latency** — time between input and output
2. **Audio quality** — block fidelity (noise floor, aliasing, THD, frequency response)
3. **Stream stability** — zero xruns, dropouts, clicks, glitches, pops
4. **Callback jitter** — stable processing time, no spikes
5. **Audio-thread CPU cost** — each block keeps or reduces its cost
6. **Zero allocation, lock, syscall, or I/O in the audio thread** — no exceptions
7. **Numerical determinism** — golden samples keep passing within tolerance

### Mandatory PR/merge checklist

If the change touches the audio thread, DSP, routing, I/O, or the block chain, the PR body or issue comment MUST answer explicitly:

- [ ] Does it affect the audio thread? Measured CPU/callback before and after? Listened ≥60s without glitches?
- [ ] Does it affect latency? What is the delta in ms? Justified?
- [ ] Does it affect any block's sound? Golden tests passing? A/B listening test done?
- [ ] Does it introduce allocation, lock, syscall, or lazy init in the hot path? If yes, revert.

### Trade-off priority (highest to lowest)

1. Sound quality AND stream stability (tied at the top)
2. Latency
3. Audio-thread CPU cost
4. Cross-platform compatibility
5. Code ergonomics / maintainability
6. New functionality

A new feature **does not justify** regressing the invariants above. Real conflicts must be discussed with the maintainer before implementation.

## Code Quality

- **Zero warnings** — `cargo build` must produce no warnings
- **Zero coupling** — blocks don't reference specific models, brands, or effect types
- **Single source of truth** — constants defined once, never duplicated
- **Separation of concerns** — business logic crates have no UI/visual config
- **No dead code** — remove unused functions, no commented-out code

See the [openrig-code-quality skill](/.claude/skills/openrig-code-quality) for the full checklist.

## Naming Conventions

- Module files prefixed: `native_`, `nam_`, `ir_`, `lv2_`
- `DISPLAY_NAME` does not contain the brand name
- `brand` field always populated in model definitions

## Build

```bash
cargo build --bin adapter-gui    # Desktop GUI
cargo build --bin adapter-console # Console mode
```
