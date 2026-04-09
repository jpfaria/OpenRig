# CLI Project Path & Auto-Save Design

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two CLI arguments to OpenRig: a project path that skips the launcher, and an `--auto-save` flag that saves on every change and hides the save button.

**Use cases:** Orange Pi service mode (fixed project, no user interaction), kiosk/embedded setups.

---

## CLI Arguments

| Argument | Example | Effect |
|----------|---------|--------|
| Project path (positional) | `openrig /path/to/project.yaml` | Opens project directly, skips launcher |
| `--auto-save` | `openrig --auto-save` | Auto-saves on every change, hides save button |
| Combined | `openrig /path/project.yaml --auto-save` | Both behaviors |

---

## Behavior

### Project path argument

- Parsed from `std::env::args()` in `main.rs` — first non-flag argument
- If path is valid and file exists → open project directly, skip launcher (`set_show_project_launcher(false)`)
- If path is invalid or file doesn't exist → log error, fall back to launcher normally (no crash)
- Passed to `run_desktop_app()` as `Option<PathBuf>`

### Auto-save flag (`--auto-save`)

- Parsed from `std::env::args()` in `main.rs` — presence of `--auto-save`
- When active, every parameter change, block toggle, or chain modification triggers an immediate save to disk
- The "save project" button is hidden in the Slint UI via a boolean property
- Passed to `run_desktop_app()` as `bool`
- Auto-save only works when a `project_path` is known (either from CLI arg or from user having opened/saved a project)

---

## Architecture

### `main.rs` — arg parsing

No external crate needed. Simple `std::env::args()` parsing:

```rust
let mut project_path: Option<PathBuf> = None;
let mut auto_save = false;

for arg in std::env::args().skip(1) {
    if arg == "--auto-save" {
        auto_save = true;
    } else if !arg.starts_with('-') {
        project_path = Some(PathBuf::from(&arg));
    }
}

run_desktop_app(runtime_mode, interaction_mode, project_path, auto_save)
```

### `lib.rs` — `run_desktop_app` signature change

```rust
pub fn run_desktop_app(
    runtime_mode: AppRuntimeMode,
    interaction_mode: InteractionMode,
    cli_project_path: Option<PathBuf>,
    auto_save: bool,
) -> Result<()>
```

### Project auto-open (lib.rs)

After window setup, if `cli_project_path` is `Some(path)`:

```rust
if let Some(path) = cli_project_path {
    if path.exists() {
        // load project and skip launcher (same logic as on_open_project_file)
        window.set_show_project_launcher(false);
    } else {
        log::error!("CLI project path does not exist: {:?}", path);
        // fall through to launcher
    }
}
```

### Auto-save (lib.rs)

- Pass `auto_save` bool into the session/callbacks
- In every callback that modifies project state (param change, block toggle, chain edit), if `auto_save` is true and `session.project_path` is `Some`, call `save_project_session()` immediately
- Wire `auto_save` as a Slint property: `window.set_auto_save(auto_save)` → hides save button

### Slint UI (`app-window.slint`)

Add to `AppWindow`:
```slint
in property <bool> auto-save: false;
```

Save button: render only when `!root.auto-save`.

---

## Files

| Action | Path |
|--------|------|
| Modify | `crates/adapter-gui/src/main.rs` |
| Modify | `crates/adapter-gui/src/lib.rs` |
| Modify | `crates/adapter-gui/ui/app-window.slint` |

---

## Out of scope

- Persistent auto-save setting in config file (always via CLI flag)
- Auto-save interval/debounce (saves immediately on every change)
- Watching the project file for external changes
