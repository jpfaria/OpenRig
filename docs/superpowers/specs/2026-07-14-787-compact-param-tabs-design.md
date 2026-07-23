# #787 — Compact view: dynamic block height + parameter tabs

Follow-up from #780, which gave the detached block editor tabbed parameters
(`parameter_groups` / `retag_for_group` / `ParamTabBar`) and a window that sizes
itself to the active tab. The compact chain view still renders every parameter
in a single right-aligned strip inside a fixed 100 px row, so a plugin with many
parameters overflows and is clipped.

## Behaviour

1. **Parameters wrap.** The parameter strip of a compact block lays its cells out
   in as many 90 px lines as it needs, and the block row grows to fit them.
   Rows keep the 12 px gap between them, so the whole list re-flows.
2. **Tabs.** When a block's parameters carry 2+ groups (manifest groups or the
   dynamic grouping from #780), a horizontal tab bar sits on top of the strip and
   only the active group's parameters are laid out. With 0 or 1 group there is no
   tab bar — the block looks exactly as it does today.
3. **Every block participates.** Blocks whose models declare a `knob_layout`
   (the curated knob overlays) wrap the same way. In practice they have few knobs
   and stay on one line, so nothing changes for them visually.
4. **A block that already fits stays 100 px tall.** Growth only happens when the
   parameters need a second line or when a tab bar is shown.
5. **EQ blocks are untouched** — they replace the strip with the dedicated EQ
   widget and keep the 100 px row.

## Geometry (single source of truth in Rust)

Slint has no flow layout, and the compact page positions rows by absolute `y`
(drag & drop, drop indicator and insert slots all do arithmetic on it). So the
layout maths lives in Rust, in one module, and Slint only consumes the result.

```
LINE_HEIGHT      = 90 px      strip line (a knob cell is 90 px tall)
BASE_ROW_HEIGHT  = 100 px     today's row
TAB_BAR_HEIGHT   =  28 px     compact tab bar
ROW_GAP          =  12 px     gap between rows (unchanged)
STRIP_BUDGET     = 720 px     nominal width available for one strip line
```

* Cell widths mirror the Slint strip (`compact_block_param_strip.slint`):
  62 px knob, 48 px bool, 110 px enum (≤ 4 options → selector knob), 140 px enum
  (dropdown). They are declared once in Rust and the Slint cells keep those as
  `preferred-width`; a narrow window shrinks the cells to their `min-width`
  instead of re-wrapping, which keeps the row height stable and the drag maths
  honest.
* Greedy wrap: cells are placed on the current line until the budget is exceeded,
  then a new line starts. Each visible parameter gets a `strip_line` index.
* `line_count` = number of lines used by the **active tab**.
* `row_height` = `TAB_BAR_HEIGHT_or_0 + 10 + line_count * LINE_HEIGHT`, floored at
  `BASE_ROW_HEIGHT`.
* `row_y` = cumulative: `ROW_GAP + Σ (previous row_height + ROW_GAP)`.

## Data flow

`CompactBlockItem` (models.slint) gains:

| field | meaning |
|---|---|
| `parameter_groups: [string]` | tab labels, first-appearance order (empty/1 → no tab bar) |
| `active_parameter_group: int` | index into the above |
| `strip_line_count: int` | lines used by the active tab |
| `row_height: length` | computed row height |
| `row_y: length` | absolute y in the flickable viewport |

`BlockParameterItem` and `BlockKnobOverlay` gain `strip_line: int` (-1 = hidden,
i.e. not in the active tab). `BlockParameterItem.tab_slot` from #780 is reused
for "is this row in the active tab", exactly as the editor grid does — the model
stays FULL so saving never drops another tab's parameters.

`build_compact_blocks` (`project_view.rs`) calls the new
`compact_block_layout::apply(...)` after building the parameter items: it derives
the groups (reusing `block_editor::parameter_groups`), re-tags for the active
group (reusing `block_editor_param_tabs::retag_for_group`), assigns `strip_line`,
and computes `strip_line_count` / `row_height` / `row_y`.

## Active-tab state

Which tab a compact block shows is view state, not project state, so it is not a
`Command`. `set_compact_blocks` is re-run on every parameter change, so the
active tab must survive the rebuild: a `RefCell<HashMap<BlockId, String>>` in the
GUI state maps a block to its active group, `build_compact_blocks` reads it, and
a new `compact-select-parameter-group(chain_index, block_index, group_index)`
Slint callback writes it and refreshes the model. A block whose stored group no
longer exists (plugin/model switched) falls back to the first group — the same
rule the editor uses.

## Slint

* `compact_block_param_strip.slint` becomes a `VerticalLayout`: an optional
  `ParamTabBar` (reused from #780) plus one right-aligned `HorizontalLayout` per
  `strip_line`, each rendering the cells whose `strip_line` matches.
* `compact_block_row.slint` anchors the header controls (footswitch, icon, label,
  model select, brand logo) to the top 100 px band instead of centring them on
  `parent.height`, so they stay put as the row grows; `height` comes from
  `block-data.row-height`.
* `compact_chain_view.slint` replaces every `bi * 112px` with the item's `row_y`
  / `row_height`: the viewport height, the row `y`, the insert slots, the drop
  indicator, and the drag drop-index (which now finds the target slot by
  comparing the dragged pointer position against the row boundaries instead of
  dividing by a constant stride).

## Testing

* Rust unit tests on the layout module (red first): wrap into lines, line count
  per active tab, row height with/without tab bar, cumulative `row_y`, a block
  that fits stays 100 px, hidden (non-active-tab) parameters get `strip_line` -1.
* Rust test on the active-tab state: rebuilding the compact model preserves the
  selected tab; a group that disappears falls back to the first.
* Slint interaction test (`i-slint-backend-testing`) proving a tab click on a
  compact row switches the rendered parameters — per the "no unverified Slint
  interaction" rule.
* Headless render (`tools/slint-render`) of a chain with a many-param VST3 block
  to check the visual before calling it done.

## Out of scope

* NAM Amp/Capture grouping — that is #786.
* Any change to the detached block editor.
