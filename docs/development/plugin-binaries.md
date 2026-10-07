# Plugin binaries (LV2 / VST3)

The open-source LV2 and VST3 plugins the installers bundle live in this repository (#1093). NAM and IR captures do not: they live in the private OpenRig-plugins repo, never ship, and reach a dev run only through a user plugin root (`OPENRIG_PLUGINS_ROOT` or **Settings → Paths → Plugins**).

## Layout

```
plugins/source/
  lv2/<package>/   manifest.yaml + data/ (TTL) + platform/<platform>/<lib>.{so,dylib,dll}
  vst3/<package>/  manifest.yaml + bundles/<Name>.vst3/Contents/{MacOS,x86_64-linux,aarch64-linux,…}/
deps/<upstream>/   git submodule pinned to a commit (see deps/DEPS.md)
```

Every package is checked by `crates/plugin-loader/tests/bundled_catalog_valid.rs`: each manifest loads, every declared file exists, and only LV2/VST3 packages are present. The release `bundle-plugins` job runs that test before uploading the tree to the platform builds.

## Git LFS

The binaries are LFS objects (`*.so`, `*.dylib`, `*.dll`, and every file under a VST3 bundle's `Contents/MacOS/` or `Contents/*-win/`, see `.gitattributes`). Pull them before running or packaging:

```bash
git lfs pull --include="plugins/source/**"
```

CI saves LFS bandwidth: test and PR jobs pull everything except `plugins/source/**`, and only the release `bundle-plugins` job pulls the plugin tree, once, for every platform.

## Building

| Piece | Role |
|---|---|
| `scripts/build-lib.sh <recipe\|all> [--platform …]` | Entry point; cross-platform builds run in Docker (`docker/Dockerfile.build-libs`); `--list` shows the recipes |
| `scripts/build-lib-internal.sh` | Dispatches a recipe inside the build environment |
| `scripts/recipes/lv2.sh`, `scripts/recipes/vst3.sh` | One `build_<recipe>` function per upstream |
| `scripts/plugin-recipes.tsv` | Which recipe produces each `plugins/source/{lv2,vst3}/<package>` |
| `scripts/add-dep.sh <name> <url> <commit>` | Registers a new upstream as a pinned submodule under `deps/` |

The submodules are not cloned by default (`scripts/solver-setup.sh` skips them too). Check out only the one you build: `git submodule update --init deps/<name>`.

## CI build

`.github/workflows/build-libs.yml` builds the binaries for linux-x86_64, linux-aarch64, macOS universal and windows-x86_64:

- tag `plugins-build-N`, `plugins-build-<recipe>-N`, `plugins-build-<platform>-N` or `plugins-build-<platform>-<recipe>-N` → artifacts only;
- `workflow_dispatch` with a recipe and a platform → artifacts, and the `commit-libs` job commits the binaries back to the dispatched branch under `plugins/source/{lv2,vst3}`.
