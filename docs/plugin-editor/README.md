# Plugin parameter editor (idea, from #879)

Status: agreed with the owner on 2026-10-07, not built yet.

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

## Open questions

- Plugins from the plugins folder (the OpenRig-plugins checkout): edit the
  `manifest.yaml` in place, or save the edit as a user override?

## Rules it must follow

- Every edit is a `Command` (GUI, MCP and gRPC parity).
- The edit only rewrites `parameters` and each capture's `values`; capture
  files never move.
- The catalog reloads after a save, like an install.
