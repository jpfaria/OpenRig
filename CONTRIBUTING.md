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
