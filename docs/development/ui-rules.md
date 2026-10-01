# UI rules — what the owner already corrected

Standing rules for any work on `.slint`, layout, components or mockups. Each one
cost a session (or several) and was stated by the owner; they are not
suggestions. Read this before touching the UI, together with `docs/gui/README.md`
(direction) and `docs/screens.md` (what each screen holds).

## 1. `PopupWindow` content does not reliably receive clicks

In this Slint version (1.16.1) a `PopupWindow` renders its content on a separate
surface that does not reliably receive dispatched pointer events — neither via
`i-slint-backend-testing` (documented in #749's own test) nor, per #761, in the
real running app. A `TouchArea` inside a `PopupWindow` can be visible and even
found by `find_by_element_id`, and clicking it silently does nothing.

Confirmed twice independently: #749 (DI loop panel select dropdown) and #761
(compact chain view's plugin/model picker, `ModelSelectWithSearch`). Both times
the popup opened and the option was visible; both times the click never fired
the callback.

**Apply:** never use `PopupWindow` for content the user has to click (search
results, option lists, menus). Render the expanded content as a normal inline
overlay hoisted to the nearest non-clipped root — a `global` singleton holding
open/position state plus a single `if global.open : …` overlay at the host
window's top level, escaping any `clip: true` ancestor. Reference
implementations: `di_panel_globals.slint` + `app-window.slint` (#749) and
`compact_chain_view.slint`'s `model-select-*` state + `CompactModelSelectOverlay`
(#761). An existing component that still uses `PopupWindow` for clickable
content is unverified until interaction-tested — `ModelSelectWithSearch`'s
default `external-popup: false` path is a known open instance, left unfixed in
#761 because those call sites were not reported broken. `PopupWindow` is still
fine for purely informational overlays (tooltips, read-only badges).

## 2. An interaction change is never shipped on a static render

`tools/slint-render` proves **layout** only — positions, spacing, colors. It says
nothing about whether a click fires, whether a popup opens or closes, whether a
`TouchArea` actually covers its target, or whether a `PopupWindow` resizes.
Shipping interaction verified by a PNG is trial and error on the owner's machine:
#749 cost four failed pushes ("não aguento mais essa tentativa e erro").

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
(options, active, placeholder, callbacks). No per-feature copy. The owner on
#749: "o select deveria ser um só para todo o sistema. a gente só deveria passar
parametros para ele. um unico componente."

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

From #398 (2026-09-29/30): the owner had to correct the logo, the brand logos and
the amp/cab borders, then "gabinete igual pedal? amp com cara de pedal?" after
they were turned into stomp boxes, and "não inventa esse tipo de coisa 'Estúdio'"
— all in one session.

## 5. An I/O label is the binding name plus the direction and channels

Any element that says which side of the I/O it is shows the **binding name plus
the direction plus the 1-based interface channels**, in the same compact row:
`GUITARRA 1 - MAIN  IN 1`, `… OUT 17,18`. Not a coloured pill, not a taller row,
and never dropping the binding name.

#1006 (2026-09-29): INPUT/OUTPUT pills grew the row → "ficou uma merda… olha o
tamanho"; dropping the name instead → "nao porra. tem que ter o nome da
ligação… preciso saber o canal tanto no IN como no OUT". He reads the rig by the
channel numbers printed on the Quantum.

**Applies to every I/O list, not just meters** (#1007, 2026-09-30): the global
mixer shipped with inputs and outputs in one list and no way to tell which was
which → "nem para vc criar uma aba para entrada uma aba para saida?". Any screen
listing endpoints separates Inputs and Outputs (tabs) and labels each strip with
name + `IN n` / `OUT n,m`. Render and check that nothing important is elided.

## 6. A chain is a graph

When the owner says the chain is a "grafo", that is a graph: arbitrary fan-out,
fan-in and output count. Do not narrow it to a tree of two-path A/B splits — and
never quote an estimate built on that narrowing as if it were a constraint of the
system. #328 (2026-10-01): "vc que inferiu isso… eu te falei que seria um grafo".

**Apply:** before modelling a structure he named, write the shape back in one
line (nodes, edges, fan-out N, fan-in N, outputs N) and get a yes.
