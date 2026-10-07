# Plugin parameter editor (#1088)

Status: agreed with the owner on 2026-10-07, being built in #1088.

## Problem

The TONE3000 install derives a plugin's parameters from the capture names.
When it cannot read a token, the leftovers become a `preset` choice with raw
names (for example `dist0 … dist10` before `dist` was added to the knob
dictionary). The user knows what the token means and the app does not.

## What the owner asked for

1. **Ask before installing.** When the inference leaves tokens it could not
   classify, the install stops and shows the capture grid. The user names
   each unknown axis (for example "Distortion", a numeric knob) before the
   package is written.
2. **Edit after installing.** Any plugin's parameters can be edited later:
   rename an axis, change it from choice to knob, rename values. This covers
   every plugin, not only TONE3000 ones.
3. **Create plugins.** Later, the user builds a plugin from scratch in the
   app: pick the captures, define the parameters, write the package.

## How the TONE3000 browser uses it

The browser (#879) only installs and updates. When an install cannot read
the capture names, it opens this editor so the user says what each token is;
the package is written after that.

## Editing in place, with a way back

- An edit rewrites the plugin's own `manifest.yaml`, wherever it lives
  (plugins folder included).
- **Recovery:** the first edit keeps the original manifest, so the user can
  restore the plugin to how it was.
- **Redo:** the user can ask the app to build the parameters again. Redo
  reads the capture names again. For a plugin from TONE3000 it first checks
  the tone's `updated_at` and downloads the captures again when TONE3000 has
  a newer version (the install stores `updated_at` for this).

## Which plugins can be edited

Only plugins that are not bundled with the app: the plugins folder
(`paths.plugins_path`), user plugins and TONE3000 installs. Bundled plugins
live inside the app (`.app/Contents/Resources`, `/usr/share/openrig`) and are
read-only; the editor never offers to change them.

## Where the editor opens

- From the plugin's info window (the one the block opens).
- From the **Plugins** window: a new top-bar button beside the TONE3000 one.
  It lists every plugin that is not bundled (bundled plugins are not shown).
  Each row offers: info, edit parameters, restore the original, redo the
  parameters and uninstall. A plugin from TONE3000 is marked as such and
  also offers **Update** when TONE3000 has a newer version.
- The list filters by block type and by origin (plugins folder, TONE3000).
- Uninstall works for every listed plugin and deletes its package folder,
  wherever it lives.
- The TONE3000 window keeps its **Installed** tab.

## Rules it must follow

- Every edit is a `Command` (GUI, MCP and gRPC parity).
- The edit only rewrites `parameters` and each capture's `values`; capture
  files never move.
- The catalog reloads after a save, like an install.
