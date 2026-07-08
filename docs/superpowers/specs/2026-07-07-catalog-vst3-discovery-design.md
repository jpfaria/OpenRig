# Catalog VST3 plugins surface as the `vst3` block type

Issue #776.

## Problem

OpenRig-plugins ships catalog VST3 packages (`manifest.yaml` with `type: vst3`,
`backend: vst3`, `bundle: bundles/<Name>.vst3`). The first is ChowCentaur
(`plugins/source/vst3/chow_centaur/`). Two things are broken:

1. The plugin-loader `BlockType` enum has no `vst3` variant, so a manifest with
   `type: vst3` fails to deserialize and the whole package is skipped.
2. Even if it parsed, the VST3 block list (`vst3_host::vst3_catalog()`) is built
   only from the system VST3 search paths (`system_vst3_paths()`), so a bundle
   living under the OpenRig plugins folder is never discovered.

A system-discovered VST3 (from the user's own `~/Library/Audio/Plug-Ins/VST3`
etc.) already works end to end: it becomes a `vst3` block, plays through a
`Vst3Processor`, and opens its native editor. A catalog VST3 must behave
**exactly the same** — same list, same block kind, same editor, same processor.

## Approach

Treat the OpenRig plugins folder as one more VST3 search location. Nothing about
the discovered path changes; it just also looks in the plugin roots.

1. **`BlockType::Vst3`** — add the variant to
   `crates/plugin-loader/src/manifest.rs` so `type: vst3` deserializes cleanly
   instead of being skipped. Update the one exhaustive match
   (`application::block_factory::block_type_to_effect_type` →
   `EFFECT_TYPE_VST3`) and the `effect_type → BlockType` helper in
   `project::catalog`.

2. **Discovery scans the plugin folder too** — `vst3_host::init_vst3_catalog`
   gains an `extra_dirs: &[PathBuf]` parameter. It scans the standard system
   paths (unchanged) **plus** those dirs, using the same light scan. The startup
   caller passes `<plugin_root>/vst3` for each configured plugin root
   (`bundled_root`, `user_root`). Entries produced there use the identical
   discovered model-ID scheme (`vst3:{bundle_stem}:{class_name}`) and flow
   through the identical `find_vst3_plugin` → `Vst3Processor` / `open_vst3_editor`
   paths — so a catalog VST3 is indistinguishable from a discovered one.

The manifest `type: vst3` keeps the loader from erroring on the package; the
block itself comes from discovery, so no duplicate drawer entry appears
(`supported_block_models("vst3")` reads only `vst3_catalog()`; nothing consumes
`packages_for(Vst3)`).

## Tests

- `plugin-loader` manifest test: `type: vst3` + `backend: vst3` deserializes to
  `BlockType::Vst3` (production-shaped ChowCentaur manifest).
- `vst3-host` integration test: scanning a dir tree containing a fixture
  `.vst3` bundle (`Contents/Resources/moduleinfo.json`) yields a catalog-shaped
  `Vst3PluginInfo` — proving the plugin folder is discovered like a system path.

## Out of scope

- No change to how the plugin's parameters are hosted (still the native editor +
  runtime `IEditController`, as the manifest comment states).
- OpenRig-plugins manifest already sets `type: vst3`; no cross-repo change here.
