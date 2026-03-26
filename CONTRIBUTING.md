# Contributing to OpenRig

## Development Workflow

Every change follows this flow:

```
Issue → Branch → Commits → PR → Review/Merge
```

### 1. Create an Issue

Every bug fix or feature must have a GitHub issue before any code is written.

```bash
gh issue create --title "Short description" --label "bug"   # for bugs
gh issue create --title "Short description" --label "feature" # for features
```

### 2. Create a Branch

One branch per issue. Branch name follows the pattern:

```bash
git checkout -b issue-{N}-short-description
```

### 3. Commit

- Commit messages in **English**
- Focus on the "why", not the "what"
- Reference the issue: `Closes #N`
- No `Co-Authored-By` lines

### 4. Push and Create a PR

```bash
git push -u origin issue-{N}-short-description
gh pr create --title "Short title" --body "Closes #N"
```

### 5. Merge Policy

| Type | Action |
|------|--------|
| **Bugfix** | Merge immediately after PR is created |
| **Feature** | PR stays open for review before merging |

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
