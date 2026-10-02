---
name: slint-best-practices
description: Use when writing, reviewing, or refactoring Slint (.slint) UI files — covers component design, property bindings, callbacks, state management, layouts, accessibility, translations, theming, and integration with Rust backend
---

# Slint Best Practices

Sources:
- [Slint Best Practices (official)](https://docs.slint.dev/latest/docs/slint/guide/development/best-practices/)
- [Slint Custom Controls Guide](https://docs.slint.dev/latest/docs/slint/guide/development/custom-controls/)
- [Slint Language Reference](https://docs.slint.dev/latest/docs/slint/)

---

# OpenRig — Slint Operational Rules

## LAW — this skill is not enough on its own: invoke the UI/UX skill first

**Before writing or changing any `.slint`, ALSO invoke `claude-plugin:ux-ui`** (design/UX). This skill covers Slint MECHANICS (bindings, layouts, `@tr`, globals); it says nothing about visual hierarchy, density, contrast, empty/error states, or whether the screen works for the user. Both are gates, not one or the other.

**Why:** invoking only the Slint skill produces a screen that compiles and renders, with visual decisions the agent invented — exactly what the UI rules forbid (never assume or invent a layout). It has happened: a whole overlay built with `slint-best-practices` alone, without the UX skill.

**How to apply:** screen work → `claude-plugin:ux-ui` + this skill, BEFORE the first line; then render with `tools/slint-render` and check the PNG before saying "done". See `docs/development/ui-rules.md`.

General UI principles (responsiveness, business/presentation separation, zero coupling) live in `openrig-code-quality`. The project's Slint-specific rules:

## File size — 500 lines per `.slint` (hard cap)

`./scripts/validate.sh` enforces ≤ 500 lines for every `.slint`. If a file goes over, **split** it before adding anything else. This is the Slint form of the "one responsibility per file" law (`docs/development/file-organization.md`).

## NEVER `sed -i` on `.slint` files

`sed -i` on macOS/BSD can **empty** a `.slint` because of encoding/locale issues (it has broken UI files before). Always use the Edit tool.

```
❌ sed -i '' 's/old/new/g' app.slint
   // DANGER: may empty the file

✅ Edit tool with old_string/new_string
```

## `@image-url()` is compile-time — no dynamic strings

`@image-url()` resolves when Slint compiles. It does not take a runtime variable. To pick an image by `model_id` or `brand`, use a ternary chain:

```slint
✅ Image {
    source: root.brand == BRAND_MARSHALL
        ? @image-url("../assets/brands/marshall/logo.svg")
        : root.brand == BRAND_VOX
        ? @image-url("../assets/brands/vox/logo.svg")
        : @image-url("../assets/brands/openrig/logo.svg");
}

❌ Image {
    source: @image-url("../assets/brands/" + root.brand + "/logo.svg");
    // FAILS: @image-url needs a compile-time string literal
}
```

The practical consequence for the OpenRig catalog: every new brand touches the ternary chain in the component that renders the logo. That is an **authorised exception** to "zero coupling" — Slint has no other way. Keep each chain in ONE component to minimise touch points: brand logos in `ui/components/brand_logo.slint`, model images in `ui/components/block_panel_brand_strip.slint`.

## Never hardcode colours or fonts per `model_id` in Slint

The principle is separation of concerns (`openrig-code-quality`). In Slint:

```slint
❌ if root.model_id == "marshall_jcm_800": Rectangle { background: #6c2a1a; }
   // WRONG: colour hardcoded in Slint per model_id

✅ private property <color> panel-bg:
       root.block-model-options[index].panel_bg;
   // RIGHT: the colour comes from the block crate's model_visual.rs (Rust), exposed as a Slint property
```

## Generic editor panel — no logic per effect_type

The block editor panel renders any effect_type from its schema, never from `if effect_type == "preamp"`. Adding a new effect_type must NOT require a change in the panel.

---

## 1. Project structure

Keep code, UI and assets in separate directories:

```
my-project/
├── src/        # business logic (Rust)
├── ui/
│   ├── app-window.slint   # entry point
│   └── components/        # reusable components
└── images/                # SVGs, PNGs, assets
```

**Rule:** no business logic inside `.slint`. Slint is declarative — computation belongs in Rust.

---

## 2. Properties — access and direction

Always declare access explicitly in components:

| Modifier | Use |
|---|---|
| `in` | Data comes from outside (parent → child) |
| `out` | Data goes outside (child → parent) |
| `in-out` | Two-way (use with care) |
| `private` | Internal to the component (default) |

```slint
component MyButton {
    in property <string> label;          // the parent sets it
    out property <bool> pressed;         // the parent observes it
    private property <bool> hovered;     // internal
}
```

**Avoid needless `in-out`** — two-way data creates coupling that is hard to trace.

---

## 3. Reactive bindings

Bindings re-evaluate automatically when their dependencies change. **Never assign by hand** what can be a binding.

```slint
// ✅ Reactive binding — updates by itself
Text { text: root.count > 0 ? "Items: \{root.count}" : "Empty"; }

// ❌ Avoid — imperative, loses reactivity
Text {
    text: "Items";
    // imperative logic through a callback that sets text = ...
}
```

**Rule:** prefer ternaries and declarative bindings over callbacks that mutate state.

---

## 4. Callbacks — direction and naming

```slint
component SearchBar {
    callback search-requested(string);    // the child notifies the parent
    callback clear-requested();

    // ❌ Avoid: a callback that returns data to the child
    // callback fetch-data() -> [DataModel]; // inverted coupling
}
```

- Callbacks flow **from child to parent** (events)
- Data flows **from parent to child** (`in` properties)
- Use `<=>` for a two-way binding between properties at the same level

---

## 5. States and animations

```slint
component Toggle {
    in-out property <bool> checked;
    private property <brush> bg: #444;

    animate bg { duration: 200ms; easing: ease-in-out; }

    states [
        on when root.checked: { bg: #4CAF50; }
    ]
}
```

- States must be **mutually exclusive** and based on logical properties
- Declare `animate` **outside** the `states` block (it applies to the transition)
- Avoid states based on negative conditions — prefer positive names

---

## 6. Layouts

```slint
// ✅ Use semantic layout components
VerticalBox {
    HorizontalBox {
        Button { text: "Cancel"; }
        Button { text: "OK"; }
    }
}

// ❌ Avoid manual x/y positioning for layouts
Rectangle {
    Button { x: 10px; y: 200px; }  // fragile, not responsive
}
```

- `VerticalBox` / `HorizontalBox` for semantic layout
- `GridLayout` + `Row` for grids
- Absolute positioning (`x`, `y`) only for overlays and decorative elements
- Prefer `preferred-width`/`preferred-height` over fixed values where possible

---

## 7. Accessibility

Declare it on **every custom interactive component**:

```slint
component CustomButton {
    in property <string> text;
    accessible-role: button;
    accessible-label: self.text;
    accessible-action-default => { clicked(); }
}
```

- `accessible-role` is required
- `accessible-label` must be human-readable text
- Tools: "Accessibility Insights" (Windows), "Accessibility Inspector" (macOS)

---

## 8. Translations

```slint
// ✅ Right — lets the translator reorder
Text { text: @tr("Hello, {}", name); }

// ❌ Wrong — concatenation makes translation hard
Text { text: @tr("Hello, ") + name; }

// ❌ Forgot the @tr
Text { text: "Save Project"; }
```

Every user-visible string uses `@tr("...")`.

---

## 9. Globals

Use `global` for state shared across components without prop-drilling:

```slint
export global AppTheme {
    out property <color> accent: #4CAF50;
    out property <length> spacing: 8px;
}

// Use from any component
Rectangle { background: AppTheme.accent; }
```

- Globals are singletons — good for theme, settings, app state
- `export` a global to use it from Rust

---

## 10. Rust integration

```rust
// Rust: read a property
let val = ui.get_my_property();

// Rust: set a property
ui.set_my_property(42);

// Rust: connect a callback
ui.on_button_clicked(|| { /* handler */ });
```

- Hyphens in Slint names become underscores in Rust (`my-prop` → `my_prop`)
- Use **weak references** in closures to avoid ownership cycles:

```rust
let ui_weak = ui.as_weak();
ui.on_clicked(move || {
    let ui = ui_weak.upgrade().unwrap();
    ui.set_count(ui.get_count() + 1);
});
```

---

## 11. Images with @image-url

`@image-url()` resolves at **compile time** — it does not take dynamic strings.

```slint
// ✅ A ternary for conditional selection
Image {
    source: root.model-id == "analog_warm"
        ? @image-url("../assets/models/analog_warm.svg")
        : @image-url("../assets/models/digital_clean.svg");
}

// ❌ Impossible — @image-url does not take a variable
// Image { source: @image-url("../assets/models/" + root.model-id + ".svg"); }
```

For many models, chain the ternaries or split components by type.

---

## 12. Naming

| Element | Convention | Example |
|---|---|---|
| Components | PascalCase | `BlockEditorPanel` |
| Properties | kebab-case | `block-type-index` |
| Callbacks | kebab-case | `block-selected` |
| Globals | PascalCase | `AppTheme` |
| States | kebab-case | `is-hovered` |

---

## 13. Common anti-patterns

| Anti-pattern | Right |
|---|---|
| Business logic in `.slint` | Compute in Rust, expose through a property |
| String literals without `@tr` | `@tr("string")` |
| Absolute `x`/`y` for layout | `VerticalBox`/`HorizontalBox` |
| Needless `in-out` | `in` or `out` by direction |
| A dynamic string in `@image-url` | Compile-time ternary chains |
| Callbacks that return data | `out` properties for data, callbacks for events |
| Rust closures without a weak ref | `ui.as_weak()` + `upgrade()` |
