# Plugin binaries (LV2 / VST3)

The open-source LV2 and VST3 plugins the installers bundle live in this repository (#1093). NAM and IR captures do not: they live in the private OpenRig-plugins repo, never ship, and reach a dev run only through a user plugin root (`OPENRIG_PLUGINS_ROOT` or **Settings → Paths → Plugins**).

## Layout

```
plugins/source/
  lv2/<package>/   manifest.yaml + data/ (TTL) + platform/<platform>/<lib>.{so,dylib,dll}
  vst3/<package>/  manifest.yaml + bundles/<Name>.vst3/Contents/{MacOS,x86_64-linux,aarch64-linux,…}/
deps/<upstream>/   git submodule pinned to a commit (see deps/DEPS.md)
```

The VST3 catalog scans the `vst3/` folder of each plugin root plus the folder of every VST3 package the plugin loader found (`crates/project/src/vst3_scan_dirs.rs`), so a dev run from the checkout, whose root is `<cwd>/plugins`, finds the bundles under `source/vst3/` as well as the packaged layout `plugins/vst3/`.

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
| `scripts/recipes/lv2.sh`, `scripts/recipes/vst3.sh`, `scripts/recipes/vst3-studio.sh` | One `build_<recipe>` function per upstream (`vst3-studio.sh`: the mixing/mastering set) |
| `scripts/plugin-recipes.tsv` | Which recipe produces each `plugins/source/{lv2,vst3}/<package>` |
| `scripts/add-dep.sh <name> <url> <commit>` | Registers a new upstream as a pinned submodule under `deps/` |

The submodules are not cloned by default (`scripts/solver-setup.sh` skips them too). Check out only the one you build: `git submodule update --init deps/<name>`.

## CI build

`.github/workflows/build-libs.yml` builds the binaries for linux-x86_64, linux-aarch64, macOS universal and windows-x86_64:

- tag `plugins-build-N`, `plugins-build-<recipe>-N`, `plugins-build-<platform>-N` or `plugins-build-<platform>-<recipe>-N` → artifacts only;
- `workflow_dispatch` with a recipe (or a comma-separated list, built as one matrix so `commit-libs` pushes once) and a platform → artifacts, and the `commit-libs` job commits the binaries back to the dispatched branch under `plugins/source/{lv2,vst3}`.

## Platform slots

Slot names in every `manifest.yaml` (`binaries:`) and in the toolchain (`scripts/build-lib.sh`, `build-libs.yml`) must match the `Lv2Slot` enum in `crates/plugin-loader/src/manifest.rs` exactly:

```
macos-universal · windows-x86_64 · windows-aarch64 · linux-x86_64 · linux-aarch64
```

Never invent or rename a slot (`windows-x64`, `windows-arm64`), and never add a serde alias to paper over one: change the enum first, then the manifests and toolchain in the same commit.

## VST3 `moduleinfo.json`

The catalog scan never loads a plugin binary: it reads each bundle's `Contents/Resources/moduleinfo.json` (class id, name, vendor) and only falls back to the `CFBundleName` of `Info.plist`, with the class id left unknown. Every shipped bundle therefore carries a `moduleinfo.json`. JUCE and VST3 SDK builds emit it; DPF (mimo) and nih-plug bundles do not, so it is committed next to the bundle, generated once from the macOS binary's plugin factory (after its `bundleEntry`). The CI merge only adds files to a bundle, so a rebuild keeps it. Without it a DPF bundle is invisible (no `CFBundleName`), and a bundle whose class name differs from its `CFBundleName` cannot be instantiated.

## LV2 `plugin_uri` = binary = TTL

OpenRig instantiates an LV2 package by walking `lv2_descriptor(i)` in the slot binary for the manifest's `plugin_uri`. Loading the manifest never opens the binary, so a stale URI passes `bundled_catalog_valid` and then fails at runtime (`LV2 plugin URI '…' not found`). An mda-lv2 rebuild once moved every URI from `moddevices.com` to `drobilla.net` and all ten `mda_*` packages broke silently.

- Rebuilt or bumped an LV2 recipe? Take `data/` from the same upstream SHA the binaries came from: the URI, port ranges and units live in the TTL.
- The `deps/<x>` pin must be the SHA the binaries were built from.
- A slot binary that does not publish the URI is worse than no binary (a missing slot is reported cleanly): drop it until a real rebuild exists.
- The binary is the source of truth; never align the manifest to a TTL without checking the binary.

The automated URI check (`qa_audit`'s `lv2_uri.rs`) stayed in OpenRig-plugins and is not ported yet; check the binary by hand (`strings <lib> | grep <plugin_uri>`) until it is.

## Updating a recipe submodule

Bump the `deps/<x>` pointer, then rebuild through `build-libs.yml` `workflow_dispatch recipe=<x> platform=all`.

- A newer upstream **tag** is the safe update: tagged releases tend to build on every platform.
- An untagged upstream HEAD is suspect: post-tag commits pull in build-system churn (DPF bumps, CMake flag changes) that has broken the macOS-universal arm64 link and the Windows slots while linux-x86_64 still built. Test one recipe at a time and be ready to revert.
- `windows-aarch64` failing on msys2 clangarm64 infra (`p11-kit.exe: Exec format error`) is optional and not a regression.
- Reverting a bump: check out the submodule to the target SHA (`git -C deps/<x> checkout <sha>`) and then `git add deps/<x>`; `git add -A` re-stages the new gitlink. `git checkout <ref> -- <dir>` does not delete slot files a partial rebuild added, so `git rm` those orphans explicitly.

## Parameter-group overlays (editor tabs)

For LV2 and VST3 the live plugin owns the parameter set (LV2: TTL control ports; VST3: `IEditController`). The manifest never declares ranges or defaults; it may declare an overlay that groups the parameters into block-editor tabs:

```yaml
backend: lv2                 # match key = the port's lv2:symbol
parameters:
  - symbol: early_level
    group: Mixer

backend: vst3                # match key = the numeric parameter id
parameters:
  - vst3_id: 1141971201
    name: output
    display_name: "Output"
    group: "General"
```

A shared `group` is one tab; tab order is first appearance; no `group` or no overlay means the app groups dynamically; an entry the live plugin does not expose is ignored. The LV2 overlay carries no `display_name` (the TTL's `lv2:name` already ships).

Count control ports per `plugin_uri`, never per bundle: an LV2 `data/` dir is a whole upstream bundle (`invada_tube`'s TTLs hold 231 control ports across 10 plugins; the one OpenRig loads has 5). Follow `lv2:port` from the manifest's `plugin_uri` and keep only `lv2:ControlPort` + `lv2:InputPort`. Only plugins with 15 or more such ports get an overlay; the rest get no `parameters: []`.
