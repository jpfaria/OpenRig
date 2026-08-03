# Release procedure — OpenRig

Step-by-step for cutting a release. *What* each tag triggers is in
[`gitflow.md`](gitflow.md) → "Release mechanics"; this file is *how* to run one.

## No clone — the whole release runs through the GitHub API

The agent never touches the main folder (CLAUDE.md → LAW ZERO) and a working
copy of this repo is expensive to create. Every step below is a `gh api` /
`gh pr` call: branch syncs use the [merges API], the tag is created as a tag
object plus a ref, and merges go through PRs. Nothing here needs a checkout.

[merges API]: https://docs.github.com/en/rest/branches/branches#merge-a-branch

## Preconditions

1. **The active `release/vX.Y.Z` carries the whole cycle.** Every issue branch
   meant for the version is merged into it. Anything still only on `develop`
   goes in via step 1; anything on an unmerged branch is *out* of the release.
2. **Tests are green on the code being shipped.** No CI runs on PRs into
   `release/*` or `main` (#862), so the evidence is `test.yml` on the `develop`
   tip the release branch carries:

   ```bash
   gh run list --repo jpfaria/OpenRig --workflow test.yml --branch develop \
     --limit 5 --json conclusion,headSha,event
   ```

   The `headSha` of the green run must be the commit the release branch merged.
3. **The milestone `vX.Y.Z` has no open issues** — `create-release` builds the
   notes from it and then closes it.
4. **Never edit `Cargo.toml`.** The tag is the version (#820); the build jobs
   write it and `commit-version-bump` pushes it to `develop` afterwards.

## 1. Sync the release branch with `develop`

The release branch drifts behind while work keeps landing on `develop`
(v0.2.0's was 23 commits behind at cut time):

```bash
gh api -X POST repos/jpfaria/OpenRig/merges \
  -f base="release/vX.Y.Z" -f head="develop" \
  -f commit_message="Merge develop into release/vX.Y.Z for the vX.Y.Z cycle"
```

Verify the branch now contains everything — `ahead_by` must be `0`:

```bash
gh api "repos/jpfaria/OpenRig/compare/release/vX.Y.Z...develop" --jq '.ahead_by'
```

A `409` means the merge conflicts; resolve it from a branch, not from the API.

## 2. Optional beta

Tag `vX.Y.Z-beta.N` on the **release branch** for a pre-release build. Same two
API calls as step 4, with `object` set to the release branch tip. The milestone
stays open and no bump is pushed.

## 3. PR the release branch into `main`, then merge it

```bash
gh pr create --repo jpfaria/OpenRig --base main --head release/vX.Y.Z \
  --title "Release vX.Y.Z" --body "…"
gh pr merge <PR> --repo jpfaria/OpenRig --merge --subject "Merge release/vX.Y.Z into main"
```

Always `--merge` — never squash or rebase; `main` must keep the cycle's history.
`gh pr checks` will report "no checks reported": expected until #862 is fixed,
which is why precondition 2 exists.

## 4. Create the annotated tag on `main`

Every OpenRig release tag is an **annotated** tag object. The REST API needs two
calls — creating only the ref would leave a lightweight tag:

```bash
MAIN=$(gh api repos/jpfaria/OpenRig/git/ref/heads/main --jq '.object.sha')
TAG_SHA=$(gh api -X POST repos/jpfaria/OpenRig/git/tags \
  -f tag="vX.Y.Z" -f message="OpenRig vX.Y.Z" -f object="$MAIN" -f type="commit" --jq '.sha')
gh api -X POST repos/jpfaria/OpenRig/git/refs \
  -f ref="refs/tags/vX.Y.Z" -f sha="$TAG_SHA"
```

Pushing the ref is what starts `release.yml`. A failed release is re-triggered
by deleting the ref (and the tag object) and recreating both at the new tip.

## 5. Back-merge `main` into `develop`

Right after tagging, while the build runs:

```bash
gh pr create --repo jpfaria/OpenRig --base develop --head main \
  --title "Back-merge main into develop after vX.Y.Z" --body "…"
gh pr merge <PR> --repo jpfaria/OpenRig --merge
```

## 6. Watch the build (~25 min)

```bash
gh run watch <run-id> --repo jpfaria/OpenRig --exit-status
```

**Only the macOS job runs today.** Linux x86_64, Linux aarch64 and Windows x64
carry a hard `if: false` since #816, so a release ships a single artifact,
`OpenRig-X.Y.Z-macos-universal.dmg`. The job list showing three "skipped" builds
is the expected state, not a failure.

## 7. Verify the outcome

```bash
gh release view vX.Y.Z --repo jpfaria/OpenRig --json isDraft,isPrerelease,assets
gh api "repos/jpfaria/OpenRig/milestones?state=all" --jq '.[] | select(.title=="vX.Y.Z") | .state'
gh api "repos/jpfaria/OpenRig/contents/Cargo.toml?ref=develop" --jq '.content' | base64 -d | grep '^version'
```

Expected: the release is public and not a draft, the milestone is `closed`
(closed by `create-release`), and `develop`'s `Cargo.toml` reads `X.Y.Z`
(pushed by `commit-version-bump`).

## 8. Open the next cycle

```bash
DEV=$(gh api repos/jpfaria/OpenRig/git/ref/heads/develop --jq '.object.sha')
gh api -X POST repos/jpfaria/OpenRig/git/refs -f ref="refs/heads/release/vX.Y+1.0" -f sha="$DEV"
gh api -X POST repos/jpfaria/OpenRig/milestones -f title="vX.Y+1.0" -f state="open"
```

Cut it **after** `commit-version-bump` has landed on `develop`, or merge
`develop` into the new release branch once it does — otherwise the next cycle
starts from a manifest one version behind. From here every issue branch is cut
from, and PR'd into, `release/vX.Y+1.0`.

## Known gaps

| Gap | Issue |
|---|---|
| No quality gate / tests on PRs into `release/*` and `main` | #862 |
| Linux and Windows release builds disabled (`if: false`) | #816 |
