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
   through `Theme.dark`. A screen is not done until it reads in both.
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
| Gear materials | `Gear.*` | real hardware (enclosures, knobs, jacks, LEDs, hub, rack, cab grille); same in both schemes |
| Weights | `line` 1px, `wire-width` 1.25px, `mark` 2px, `track` 3px | every stroke |
| Spacing | `spaces.small/medium/large/xlarge` (10/14/20/28px) | gaps and padding |

A tint is a token with `.with-alpha()`, `.brighter()` or `.darker()`, never a
new hex. A monochrome SVG icon always carries a `colorize` from a token.

## Typography

Bebas Neue is the face (do not propose another). Category and section labels
are upper case, bold (700) with 1.5–2px letter spacing; names and values are
600; body text is 400. One size: `Theme.min-font`, with hierarchy from
weight, case and colour (`ink` → `ink-2` → `ink-3`), not from small text.

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
8px dot. A bypassed block is grey and dimmed, never hidden.

## Graph view

Spec of the canvas itself: `docs/gui/graph-view.md`. The visual rules:

- The canvas is a `Theme.well` with the `dot` grid.
- Wires are `Theme.wire-width` curves that end on the edge of the gear, the
  rim of a jack or a port of a hub (`GraphPorts`); nothing is drawn under a
  translucent piece.
- Each split path has its lane colour (`GraphLanes`: path 0 accent, then
  `c-rvb`, `c-mod`, `c-dly`); the trunk is `Theme.wire`.
- Split and mixer are the `GraphHub`: a slate tile with one port and one
  letter per path, any number of paths.
- Under a block: the power disc and the category in its colour, then the
  model name. The first block of the outer paths carries a `PATH A` / last
  path tag in the lane colour.
- Input/output are jacks with IN / OUT under them; a port block is named
  after its binding.

## Overlays and paint order

A bar whose hover labels hang below it (chain header, top bar, compact
header, block editor header) declares `z: 10` so its tooltip paints over the
page below (`src/issue_398_paint_order_tests.rs`). A list a user clicks is a
root-level overlay, never a `PopupWindow` (`ui-rules.md` §1).

## Checking a screen

1. Render the component with `tools/slint-render` (standalone mock, both
   schemes) and look at the PNG.
2. Open the app from the solver with an isolated `--config` / `--project`,
   capture the window, and compare it with the T-numbered screen of the
   mockup side by side, in light and dark. The agent does this, not the
   owner (#398).
3. Anything that differs is fixed before the push, or written on the issue
   as an open item.
