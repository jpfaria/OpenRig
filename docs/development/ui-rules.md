# UI rules — what the owner already corrected

Standing rules for any work on `.slint`, layout, components or mockups. Each one
cost a session (or several) and was stated by the owner; they are not
suggestions. Read this before touching the UI, together with `docs/screens.md`
(what each screen holds).

## 0. Before any UI work

Invoke `claude-plugin:ux-ui` and `slint-best-practices` first. Never assume a
layout: render it with `tools/slint-render` (headless PNG) and look at the PNG
before saying "done", then close the visual in a short loop with the owner.
Icons are SVG via `@image-url` + colorize, never a glyph (it renders as tofu on
the Orange Pi). Bebas Neue is the default font by choice — do not propose
changing it. Keep the look consistent across screens.

## 1. `PopupWindow` content does not reliably receive clicks

In this Slint version (1.16.1) a `PopupWindow` renders its content on a separate
surface that does not reliably receive dispatched pointer events — neither via
`i-slint-backend-testing` nor in the real running app. A `TouchArea` inside a
`PopupWindow` can be visible and even found by `find_by_element_id`, and
clicking it silently does nothing: the popup opens, the option is visible, and
the click never fires the callback.

**Apply:** never use `PopupWindow` for content the user has to click (search
results, option lists, menus). Render the expanded content as a normal inline
overlay hoisted to the nearest non-clipped root — a `global` singleton holding
open/position state plus a single `if global.open : …` overlay at the host
window's top level, escaping any `clip: true` ancestor. Reference
implementations: `di_panel_globals.slint` + `app-window.slint` and
`compact_chain_view.slint`'s `model-select-*` state + `CompactModelSelectOverlay`.
An existing component that still uses `PopupWindow` for clickable
content is unverified until interaction-tested — `ModelSelectWithSearch`'s
default `external-popup: false` path is a known open instance. `PopupWindow` is still
fine for purely informational overlays (tooltips, read-only badges).

## 2. An interaction change is never shipped on a static render

`tools/slint-render` proves **layout** only — positions, spacing, colors. It says
nothing about whether a click fires, whether a popup opens or closes, whether a
`TouchArea` actually covers its target, or whether a `PopupWindow` resizes.
Shipping interaction verified by a PNG is trial and error on the owner's machine.

The gotchas that only a real event reveals: a `ComboBox` dropdown is a SECOND
`PopupWindow` and Slint keeps only one open, so opening it closes the parent; a
`PopupWindow` does not resize after `show()`; row clicks inside a `Flickable` in
a `close-on-click-outside` popup can be swallowed or dismiss the popup.

**Apply:** for any `.slint` change touching a `TouchArea`, `PopupWindow`, list
selection or dropdown, write or extend a headless interaction test before
pushing. Pattern in `crates/adapter-gui/src/io_bindings_ui_tests.rs`:
`i_slint_backend_testing::init_no_event_loop()`, host the component in a real
exported `Window`, locate with `ElementHandle::find_by_element_id` /
`find_by_accessible_label`, dispatch real `WindowEvent::PointerPressed` /
`PointerReleased` at the element centre, assert the callback fired or the state
changed. See it green, then push. If the tester cannot reach the content, that
is itself the signal to pick a non-popup, testable design — say so instead of
shipping blind.

## 3. One select component for the whole app

There is a SINGLE reusable select/dropdown component; features pass parameters
(options, active, placeholder, callbacks). No per-feature copy.

The app already grew near-duplicates (`PresetSelect`, `ModelSelectWithSearch`,
an ad-hoc DI picker). Copies drift in styling, click reliability and search
behaviour, each has to be fixed separately, and the UI becomes inconsistent.

**Apply:** a feature that needs a select reuses the shared component (extract one
from the proven `preset_select.slint` if it does not exist yet) and passes data
plus callbacks. The shared component owns the field, search, list,
selected-highlight and overlay; the consumer owns only its filter state (Slint
has no string `contains`, so filtering stays in Rust per consumer via a small
global, mirroring `PresetPicker`). Re-point existing selects at it, proving it
against the working preset select first. The trigger may differ per consumer (a
wide field vs a compact icon); the dropdown does not.

**A choice among data-driven options is a select, never a horizontal row of
buttons.** Endpoints, presets, sources: their count and label length come from the
user's rig, so a segmented row overflows or truncates and reads as a list laid on
its side. A segmented control is only for a short, fixed set the app defines
(½× / 1× / 2×).
The reverse holds too: a short, fixed set the app defines (channel mode
Mono / Stereo / Dual mono, sample rate, buffer size) is a segmented control
(radio), never a select: a select hides two or three known options behind a
click for no reason. Inside a panel that is itself an overlay, the select's list is a
root-level modal (`looper_endpoint_picker.slint`, `looper_preset_picker.slint`).

**An input/output list shows each physical endpoint once.** I/O bindings overlap:
the same guitar input or the same MAIN output (same device + channels) is
declared by several of them. Every list the user picks from (looper selects, DI
output, metronome and drums outputs, the graph's input/output checklists) is
built from `domain::distinct_endpoints`: one row per physical endpoint, labelled
with the endpoint name, prefixed with the binding name only when two different
endpoints share that name. A pick persists the first binding copy, a saved copy
of any other binding still resolves to the same row, and a checklist toggle
switches every copy. The binding editor in Settings is the one place that lists
per binding.

## 4. A mockup restyles the real screen — it never invents UI

In a redesign concept, only restyle what the screen actually has: read the real
screenshot (`docs/assets/sc*.png`) first and keep exactly those elements. Never
add invented labels or controls (an "Estúdio" crumb, a CPU/latency pill, a
"+ Chain" button, extra scenes, meters in the chain header) and never change the
OpenRig logo or the brand logos in `assets/brands`.

"Same style as the pedals" means the same FINISH — colour, gradient, shadow, no
frame — not the pedal SHAPE: an amp still looks like an amp head, a cab like a
cabinet, a rack unit like a rack unit. Keep it uncluttered: no decorative screws,
scanlines or ticks. UX wins over decoration.

**Lines stay subtle.** The owner wants a fine, quiet look: 1px low-contrast
dividers, line icons at about 1.5px on a 24px grid, knob arcs and graph wires
under 2px, fader and slider tracks about 3px, accent bars 2px at most. Heavy
borders, thick tracks and dark outlines read as coarse.

## 5. An I/O label is the binding name plus the direction and channels

Any element that says which side of the I/O it is shows the **binding name plus
the direction plus the 1-based interface channels**, in the same compact row:
`GUITARRA 1 - MAIN  IN 1`, `… OUT 17,18`. Not a coloured pill, not a taller row,
and never dropping the binding name. The owner reads the rig by the channel
numbers printed on the Quantum.

**Applies to every I/O list, not just meters:** any screen listing endpoints
separates Inputs and Outputs (tabs) and labels each strip with
name + `IN n` / `OUT n,m`. Render and check that nothing important is elided.

## 6. A chain is a graph

When the owner says the chain is a "grafo", that is a graph: arbitrary fan-out,
fan-in and output count. Do not narrow it to a tree of two-path A/B splits — and
never quote an estimate built on that narrowing as if it were a constraint of the
system.

**Apply:** before modelling a structure he named, write the shape back in one
line (nodes, edges, fan-out N, fan-in N, outputs N) and get a yes.

## 7. Reading a headless render

`tools/slint-render` captures the **first frame**. Any property under an
`animate` block is therefore rendered at its *starting* value, not its
settled one: an `animate colorize` makes icons come out dark and muddy, an
`animate background` gives buttons an off colour, and it reads exactly like a
contrast bug that is not there. When a rendered component looks washed out,
check for `animate` before redesigning it. Prefer no animation on the properties a render has to
prove (state colour, enabled/disabled), and keep animations for hover
transitions the PNG does not need to show.

Two more traps the same render caught, worth checking first:

- A fixed-size button (`width`/`height`) inside a `HorizontalLayout` still
  gets stretched, and `min-*`/`max-*` conflict with `width`/`height`
  (a hard compile error). Icon clusters in this app are positioned
  absolutely for that reason; use layouts for the parts that stretch.
- Texts inside a stretching container inflate it with their intrinsic width
  and push the trailing buttons out of the card. Give that container a
  `min-width` and `clip: true`.
