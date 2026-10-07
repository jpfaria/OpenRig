# Visual language (#398)

The look every OpenRig screen follows since #398. Any new screen, dialog,
overlay or control is built from what is written here, and reuses the
components listed below instead of drawing its own. When this page and the
code disagree, the code in `crates/adapter-gui/ui/theme.slint` wins for a
value and this page wins for a rule; fix whichever is wrong in the same
commit.

Read with `docs/development/ui-rules.md` (the owner's standing UI rules) and
`docs/screens.md` (what each screen holds).

## Reference

- **The approved mockup** is `docs/gui/visual-language/mockup/index.html`
  (v17), a static page with every screen numbered T01–T42 in its top-left
  corner, in light and dark. Open it in a browser and use the theme toggle.

  | Screens | Section |
  |---|---|
  | T01–T02 | Launcher |
  | T03 | Chains (graph view) |
  | T04 | Compact view |
  | T05 | Block editor |
  | T06–T07 | Mixer |
  | T08–T11 | Tuner · Spectrum · Metronome |
  | T12–T14 | Drums · Backing tracks |
  | T15–T21 | Settings |
  | T22–T24 | Chain · Insert · Port editors |
  | T25–T35 | Popups (dialogs, pickers, checklists) |
  | T36–T40 | DI · Looper · Tone Doctor |
  | T41–T42 | Plugin info · graphic EQ |

- A new screen that has no T-number is drawn in the same language: take the
  nearest numbered screen as the template (a tool window → T08–T14, a dialog
  or picker → T25–T35, a settings page → T15–T21) and change only the
  content.
- The mockup restyles the screens the app has. It never adds controls; see
  `ui-rules.md` §4.

## Principles

1. **Quiet and fine.** 1px hairlines, wires and knob arcs under 2px, tracks
   about 3px, accent marks 2px at most. No heavy borders, no dark outlines,
   no decorative screws, scanlines or ticks.
2. **Hardware is drawn, the app is flat.** A block is drawn as its piece of
   gear (pedal, amp head, cab, rack unit, expression pedal); the readouts are
   LCD glass. Everything else (pages, panels, fields, lists) is a flat
   surface with a hairline.
3. **Both schemes, always.** The app follows the system light/dark scheme
   through `Theme.dark`, unless Settings → Appearance forces Light or Dark
   (`Theme.mode`, set by Rust on every window). A screen is not done until
   it reads in both; flip the setting to check.
4. **Colour carries meaning.** The accent marks the active/selected thing;
   the category colours (`c-*`) mark a block's type everywhere (chip, graph
   caption, compact row, editor); `in` / `out` mark I/O; `ok` / `warn` /
   `bad` mark state; `b-*` mark the plugin backend. Never a colour for
   decoration only.
5. **Text never goes below `Theme.min-font` (18px).** The only exceptions
   are the text printed on gear around a knob (`knob-caption-font`,
   `knob-option-font`). Long names elide; they are never clipped.

## Tokens

Every colour, weight and size comes from `ui/theme.slint`; no raw hex
anywhere else (`ui-rules.md` §0.1).

| Group | Tokens | Use |
|---|---|---|
| Surfaces | `page` → `panel-lo` / `panel` / `panel-hi`, `well`, `field` | page behind everything, raised panels, the sunken well of a canvas or tool body, input fields |
| Chrome | `chrome`, `chrome-2`, `chrome-line`, `chrome-fg(-2)`, `chrome-field(-line)`, `chrome-ring`, `chrome-ok/warn/bad` | the dark bars (top bar, chain header, tool bar) in both schemes |
| Lines | `hair`, `hair-2`, `field-line`, `groove`, `ktrack`, `wire`, `dot` | dividers, field outlines, slider grooves, knob tracks, graph wires, the graph dot grid |
| Text | `ink`, `ink-2`, `ink-3`, `on-accent`, `hero-fg` | strongest to faintest; text on an accent fill; text on a red/coloured fill |
| Overlays | `scrim`, `hover`, `shadow` | modal scrim, hover tint, drop shadows |
| Accent / state | `accent(-soft/-line)`, `ok`, `warn`, `bad` (+ `-soft`), `in`, `out` | selection and active state; status; I/O direction |
| Categories | `c-filter`, `c-dyn`, `c-gain`, `c-amp`, `c-pre`, `c-cab`, `c-mod`, `c-dly`, `c-rvb`, `c-vol` | the type colour of a block, everywhere it shows |
| Backends | `b-nam`, `b-native`, `b-ir`, `b-lv2`, `b-vst3` | the backend badge of a model |
| Fixed looks | `hero-*`, `lcd-*`, `scope-*` | the launcher hero, the amber LCDs, the teal spectrum scope; same in both schemes |
| Gear materials | `Gear.*` | real hardware (enclosures, knobs, jacks, LEDs, rack, cab grille); same in both schemes |
| Routing hub | `hub-hi` / `hub-lo` / `hub-edge` / `hub-line`, `port-fill` / `port-ring` | the split/mixer node: the app's own, so it follows the scheme (slate in dark, pale with dark ink in light) |
| Weights | `line` 1px, `wire-width` 1.25px, `mark` 2px, `track` 3px | every stroke |
| Spacing | `spaces.small/medium/large/xlarge` (10/14/20/28px) | gaps and padding |

A tint is a token with `.with-alpha()`, `.brighter()` or `.darker()`, never a
new hex. A monochrome SVG icon always carries a `colorize` from a token.

## Typography

Three faces, as in the approved mockup (owner's decision, #398):

- **Barlow** is the UI face: every label, name, value, button, list row and
  body text. Windows bind `default-font-family: Locale.font-family`, which
  Rust sets to Barlow for a Latin locale (`locale_font.rs`).
- **Bebas Neue** is the display face, only for what the mockup sets in it:
  window and page titles (`ToolBar`, `DialogCard`, settings page heading,
  top bar), the chain name, accordion sections (`SectionToggle`), the
  model word of the block editor, category tabs, and the big readouts (tuner
  note, BPM, tap, player time, drum bar). Such a text sets
  `font-family: Locale.display-font-family`, never the face by name: a CJK
  or Devanagari locale gets its own script face there too, so nothing
  renders as tofu.
- **JetBrains Mono** (Medium) is the readout face, only where the mockup sets
  `var(--mono)`: knob and EQ values, mixer dB readouts, the latency badge,
  the app version, channel labels in the channel picker, Tone Doctor metric
  values, looper times and slider values, preset numbers, the rack screen of
  gear art, the tuner and spectrum LCD captions and scales, the tuner octave,
  the BPM caption, the player loop range, project paths in the launcher and
  the project file field. Such a text sets
  `font-family: Locale.mono-font-family`; a CJK or Devanagari locale keeps
  its own script face there as well.

Barlow prints the case it is given. Labels the mockup sets in upper case
(`FieldCaption`, `TagPill`, the graph's category caption) apply
`.to-uppercase()` in the component, so callers pass the normal translated
string. A knob's caption is printed as the model names it ("Treble",
"Level to A"), never upper-cased. Labels are bold (700) with 1.5–2px letter spacing; names and values
are 600; body text is 400. One size: `Theme.min-font`, with hierarchy from
weight, case and colour (`ink` → `ink-2` → `ink-3`), not from small text.
The fonts live in `ui/fonts/Barlow/` and `ui/fonts/JetBrainsMono/`, each with
its licence (`OFL.txt`).

## Components to reuse

Build from these; a near-duplicate is a bug (`ui-rules.md` §3).

| Need | Component |
|---|---|
| Modal dialog | `DialogCard` (card over the scrim) + `DialogFoot` (button row) + `FormButton` |
| Form field | `FieldCaption` above `TextField`, `Select` (data-driven list) or `SegControl` (short fixed set, e.g. Mono / Stereo / Dual mono, sample rate, buffer size) |
| On/off | `ToggleSwitch` (settings, forms); `PillSwitch` (footswitch pill on a tool bar); `BlockPowerButton` (a block's power disc) |
| Check list row | `CheckMark` inside a `ListBox` |
| Tool window (tuner, metronome, drums, player, spectrum, mixer, looper, DI, Tone Doctor) | `ToolBar` + `ToolBarClose` over a body on `Theme.well`; readouts in `LcdGlass` |
| Knob | `PanelKnob` / `KnobArc`; a stepped choice is `SelectorKnob` |
| Small button on a dark bar | `BarButton`; header icons use `HeaderIconStyle` and `IconTooltip` |
| Action an icon says alone (save, delete, add, cancel, close, refresh, play) | `FormButton` / `EditorButton` / `PanelActionButton` with `icon-only: true` and an `icon`; the `label` stays as hover label and accessible label (`ui-rules.md` §8) |
| Hover label | `IconTooltip` inside the control; the window's `HoverTipLayer` draws it |
| Placing an overlay | `OverlayPlacement` (`top`, `top-above`, `left`, `fit`) against the window size; a `PopupWindow` reads it from `WindowBounds` (`ui-rules.md` §9) |
| Parameter tabs | `ParamTabBar`; a row wider than the bar scrolls sideways and its right edge fades into `fade-into` while more tabs wait there |
| Tag / badge | `TagPill` |
| Settings item | `SettingsCard` |
| Transient message | `Toast` |
| Block icon / brand | `EffectTypeIcon`; `BrandLogo` (draws nothing when the brand has no logo, and the caller then shows `EffectTypeIcon`) |
| A block drawn as gear | `GearArt`, sized by `GearShape` |

## Gear

`GearShape.piece(icon-kind)` picks the piece and its unscaled size; `GearArt`
draws it. Anything that ends on the gear's edge (a graph wire, a selection
ring) reads the same `GearShape`, so the two never disagree.

| Piece | Block types | Size (px) |
|---|---|---|
| stomp | drive, dynamics, modulation, delay, filter, … | 58 × 84 |
| head | amp, preamp, full rig, NAM | 104 × 58 |
| cab | cab, IR, body | 78 × 80 |
| rack | reverb, insert | 104 × 40 |
| exp | wah, utility | 46 × 80 |

The enclosure is tinted with the block's category colour, the window/screen
prints the model name (elided), the knobs are drawn caps, the LED is a drawn
8px dot. An amp head is the brand logo on the white upper panel over a band
of the category colour across its lower 20px, with no knobs. A brand logo is
always monochrome in the tint it is given (`BrandLogo`), and its SVG
`viewBox` is cropped to the letters so it fills its box. A bypassed block is
grey and dimmed, never hidden.

## Graph view

Spec of the canvas itself: `docs/gui/graph-view.md`. The visual rules:

- The canvas is a `Theme.well` with the `dot` grid.
- Wires are `Theme.wire-width` curves that end on the edge of the gear, the
  rim of a jack or a port of a hub (`GraphPorts`); nothing is drawn under a
  translucent piece.
- Each split path has its lane colour (`GraphLanes`: path 0 accent, then
  `c-rvb`, `c-mod`, `c-dly`); the trunk is `Theme.wire`.
- Split and mixer are the `GraphHub`: a tile in the scheme's hub colours with one port and one
  letter per path, any number of paths.
- Under a block: the power disc and the category in its colour, then the
  model name. A path is told apart by its lane colour only, never by a
  written `PATH A` (#398); in the compact view the lane bar is the split or
  mix icon and a line, both in the lane colour.
- Input/output are jacks with IN / OUT under them; a port block is named
  after its binding.

## Overlays and paint order

Nothing leaves the window (`ui-rules.md` §9). Every window declares
`WindowBoundsProbe { }` and `HoverTipLayer { }`: the probe tells its
`PopupWindow`s where the window's edges are, and the layer draws every hover
label of the window above everything, so no dialog clip, scroll view or
later sibling can cover or cut it. `IconTooltip` only hands its text and its
control's position to that layer; it draws nothing in place. A bar whose
hover labels hang below it (chain header, top bar, compact header, block
editor header) still declares `z: 10` (`src/issue_398_paint_order_tests.rs`).
A list a user clicks is a root-level overlay, never a `PopupWindow`
(`ui-rules.md` §1).

The hover card of a graph block is a window-level layer too: the canvas
writes the hovered block and its window position into `BlockHoverState`,
and `BlockHoverLayer`, declared last in the window, draws the card clamped
inside the window. Drawn inside the canvas, the chain header above it
painted over the card.

## Checking a screen

1. Render the component with `tools/slint-render` (standalone mock, both
   schemes) and look at the PNG.
2. Open the app from the solver with an isolated `HOME` and a copied
   `--project` (`--config` alone still writes the owner's config; see
   `docs/cli.md`),
   capture the window, and compare it with the T-numbered screen of the
   mockup side by side, in light and dark. The agent does this, not the
   owner (#398).
3. Anything that differs is fixed before the push, or written on the issue
   as an open item.
