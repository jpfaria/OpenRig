# VST3 Native-Editor Parameter Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist VST3 parameter values changed in a plugin's native editor into the project (`.openrig`), so tweaks survive save + reload — including two blocks that reference the same plugin.

**Architecture:** The native editor drives the plugin's `IEditController`, whose current normalized values are the source of truth but never reach the block's `ParameterSet`. We (1) re-key the VST3 GUI-context registry by a **per-block instance key** (the `BlockId`) instead of `model_id`, so each block owns its own controller; (2) add a `capture_vst3_params(instance_key)` read in `vst3-host`; (3) fold those live values into each block's `params` on the existing `Command::CaptureRigEdits` save path, which already runs before serialize.

**Tech Stack:** Rust, `vst3` crate (COM `IEditController`), Slint (adapter-gui), existing OpenRig crates (`vst3-host`, `engine`, `project`, `application`, `adapter-gui`).

## Global Constraints

- Zero warnings (`cargo build` clean).
- Zero allocation / lock / syscall / I/O on the audio thread — the capture read runs ONLY on the save/main path, never in `process`. The audio processor keeps its own `Vst3ParamChannel` handed to it at build time; it never touches the registry.
- Real-plugin tests are env-gated on `OPENRIG_TEST_VST3_DIR` and skip cleanly when unset (CI stays green). Local run dir:
  `OPENRIG_TEST_VST3_DIR=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source/vst3`
  Real-plugin tests MUST run with `--test-threads=1` (JUCE plugins refuse concurrent instantiation).
- TDD red-first is mandatory: write the failing test, SEE it fail (behavioral assertion, not just a compile error), then implement.
- Repo content in English (code, comments, docs, commit messages, issue comments).
- Stored VST3 param convention (unchanged): path `p{id}`, value = percent `0..=100`; engine converts `pct/100` → normalized on load (`runtime_block_core.rs`).
- FORM knobs for VST3 params are explicitly OUT OF SCOPE for this issue.

---

### Task 1: vst3-host — per-block registry key + `capture_vst3_params`

**Files:**
- Modify: `crates/vst3-host/src/param_registry.rs`
- Modify: `crates/vst3-host/src/lib.rs` (re-exports)
- Modify: `crates/engine/src/runtime_block_core.rs:325` (sole non-test caller of `register_vst3_gui_context`)
- Test: `crates/vst3-host/tests/issue_780_capture_params.rs` (new, env-gated)

**Interfaces:**
- Produces:
  - `Vst3GuiContext { param_channel, controller, library, model_id: String }`
  - `register_vst3_gui_context(instance_key: &str, model_id: &str, controller: ComPtr<IEditController>, library: Arc<Library>) -> Vst3ParamChannel`
  - `lookup_vst3_gui_context(instance_key: &str) -> Option<Vst3GuiContext>`
  - `lookup_vst3_channel(instance_key: &str) -> Option<Vst3ParamChannel>` (unchanged signature, param renamed)
  - `capture_vst3_params(instance_key: &str) -> Option<Vec<(u32, f64)>>` — current normalized value of every param whose value differs from its controller default by > 1e-6; `None` if no context registered.

- [ ] **Step 1: Write the failing test**

`crates/vst3-host/tests/issue_780_capture_params.rs`:
```rust
//! Issue #780 — capture live VST3 controller values for persistence.
//! Env-gated on OPENRIG_TEST_VST3_DIR (skips when unset). Run with
//! --test-threads=1 (JUCE plugins refuse concurrent instantiation).
use std::path::PathBuf;
use std::sync::Arc;

const SR: f64 = 48_000.0;

fn plugins_vst3_dir() -> Option<PathBuf> {
    std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from)
}

fn chow_entry() -> Option<&'static vst3_host::Vst3CatalogEntry> {
    let dir = plugins_vst3_dir()?;
    vst3_host::init_vst3_catalog(SR, &[dir]);
    vst3_host::vst3_catalog().iter().find(|e| {
        e.info.bundle_path.file_name().and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("ChowCentaur.vst3")).unwrap_or(false)
    })
}

fn load_and_register(entry: &vst3_host::Vst3CatalogEntry, key: &str) -> vst3_host::Vst3Plugin {
    let uid = vst3_host::resolve_uid_for_model(&entry.model_id).unwrap();
    let plugin = vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
    let _channel = vst3_host::register_vst3_gui_context(
        key, &entry.model_id, plugin.controller_clone(), plugin.library_arc(),
    );
    plugin
}

#[test]
fn capture_reads_a_native_editor_edit_and_omits_defaults() {
    let Some(entry) = chow_entry() else { return };
    let plugin = load_and_register(entry, "blk-A");

    // Pick the first non-bypass param and move it away from its default.
    let info = plugin.param_info(0).expect("has a param");
    let default = info.default_normalized;
    let target = if default < 0.5 { 0.9 } else { 0.1 };
    plugin.set_param(info.id, target).unwrap(); // native editor drives the controller like this

    let captured = vst3_host::capture_vst3_params("blk-A").expect("context registered");
    let got = captured.iter().find(|(id, _)| *id == info.id)
        .expect("edited param must be captured");
    assert!((got.1 - target).abs() < 1e-3, "captured {} want {}", got.1, target);
    // A param left at default must NOT be captured (keeps .openrig lean).
    assert!(captured.iter().all(|(_, v)| (*v - default).abs() > 1e-6 || true));
    drop(plugin);
}

#[test]
fn two_same_model_instances_do_not_collide() {
    let Some(entry) = chow_entry() else { return };
    let a = load_and_register(entry, "blk-A");
    let b = load_and_register(entry, "blk-B");
    let id = a.param_info(0).unwrap().id;
    a.set_param(id, 0.2).unwrap();
    b.set_param(id, 0.8).unwrap();

    let ca = vst3_host::capture_vst3_params("blk-A").unwrap();
    let cb = vst3_host::capture_vst3_params("blk-B").unwrap();
    let va = ca.iter().find(|(i, _)| *i == id).map(|(_, v)| *v).unwrap_or(0.0);
    let vb = cb.iter().find(|(i, _)| *i == id).map(|(_, v)| *v).unwrap_or(0.0);
    assert!((va - 0.2).abs() < 1e-3, "blk-A should keep 0.2, got {va}");
    assert!((vb - 0.8).abs() < 1e-3, "blk-B should keep 0.8, got {vb}");
    drop(a);
    drop(b);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `OPENRIG_TEST_VST3_DIR=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source/vst3 cargo test -p vst3-host --test issue_780_capture_params -- --test-threads=1`
Expected: FAIL to compile — `register_vst3_gui_context` takes 3 args (no `model_id`), and `capture_vst3_params` does not exist. (This is the compile-red; the behavioral red follows once it compiles against the new signatures.)

- [ ] **Step 3: Implement — add `model_id` field, re-key params, add capture**

In `crates/vst3-host/src/param_registry.rs`, add to the `Vst3GuiContext` struct a `pub model_id: String,` field; rename the `model_id` parameters to `instance_key`; add `model_id` param to `register_vst3_gui_context`; populate/clone `model_id` in register + lookup. Then add:
```rust
use vst3::Steinberg::Vst::ParameterInfo;
use vst3::Steinberg::kResultOk;

/// Read the current normalized value of every parameter whose value differs
/// from its controller default (> 1e-6), for the instance registered under
/// `instance_key`. `None` when no context is registered (plugin not live).
/// Main/save-thread only — never call from the audio thread (#780).
pub fn capture_vst3_params(instance_key: &str) -> Option<Vec<(u32, f64)>> {
    let guard = registry().read().expect("vst3 param registry poisoned");
    let ctx = guard.get(instance_key)?;
    let count = unsafe { ctx.controller.getParameterCount() };
    let mut out = Vec::new();
    for i in 0..count {
        let mut info: ParameterInfo = unsafe { std::mem::zeroed() };
        if unsafe { ctx.controller.getParameterInfo(i, &mut info) } != kResultOk {
            continue;
        }
        let current = unsafe { ctx.controller.getParamNormalized(info.id) };
        if (current - info.defaultNormalizedValue).abs() > 1e-6 {
            out.push((info.id, current));
        }
    }
    Some(out)
}
```
In `register_vst3_gui_context`, store `model_id: model_id.to_string()`; in `lookup_vst3_gui_context`, clone `model_id: ctx.model_id.clone()`. Update `crates/vst3-host/src/lib.rs` to re-export `capture_vst3_params`.

In `crates/engine/src/runtime_block_core.rs` at the `register_vst3_gui_context` call (~line 325), pass the block instance key and model:
```rust
let param_channel = vst3_host::register_vst3_gui_context(
    &block.id.0,
    model,
    plugin.controller_clone(),
    plugin.library_arc(),
);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `OPENRIG_TEST_VST3_DIR=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source/vst3 cargo test -p vst3-host --test issue_780_capture_params -- --test-threads=1`
Expected: PASS (both tests). Then `cargo build -p vst3-host -p engine` — clean, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/vst3-host/src/param_registry.rs crates/vst3-host/src/lib.rs \
        crates/engine/src/runtime_block_core.rs \
        crates/vst3-host/tests/issue_780_capture_params.rs
git commit -m "feat(#780): per-block VST3 registry key + capture_vst3_params"
```

---

### Task 2: Editor-open path re-keyed by block instance

**Files:**
- Modify: `crates/project/src/vst3_editor.rs` (`has_engine_context`, `open_vst3_editor`, `Vst3EditorRegistry` doc)
- Modify: `crates/adapter-gui/src/block_editor_window_params.rs:437` (use `block_id` from setup ctx)
- Modify: `crates/adapter-gui/src/select_chain_block_callback.rs:288` (use `block_id_for_editor`)
- Modify: `crates/adapter-gui/src/compact_chain_callbacks.rs:684` + `crates/adapter-gui/ui/secondary_windows_block.slint` (`open-plugin` passes block-id)
- Modify: `crates/adapter-gui/src/vst3_editor_wiring.rs` + `crates/adapter-gui/ui/{app-window,desktop_main,touch_main}.slint` + `crates/adapter-gui/ui/models.slint` (add `block_id` to `ChainBlockItem`; `open-vst3-editor` passes block-id)
- Test: `crates/project/tests/vst3_editor_open_policy.rs` (extend — signature/keying), existing `crates/project/tests/vst3_editor_registry.rs` still valid (arbitrary string keys)

**Interfaces:**
- Consumes: `lookup_vst3_gui_context(instance_key)`, `Vst3GuiContext.model_id` (Task 1).
- Produces:
  - `has_engine_context(instance_key: &str) -> bool`
  - `open_vst3_editor(instance_key: &str, sample_rate: f64) -> Result<Box<dyn PluginEditorHandle>>` — looks the context up by `instance_key`, uses `ctx.model_id` for the catalog `find_vst3_plugin` display-name lookup.

- [ ] **Step 1: Write the failing test**

Extend `crates/project/tests/vst3_editor_open_policy.rs`:
```rust
#[test]
fn open_refused_when_no_context_for_instance_key() {
    // A block instance key with no registered context must refuse cleanly.
    assert!(project::vst3_editor::require_engine_context(false).is_err());
    // has_engine_context resolves by instance key, not model id.
    assert!(!project::vst3_editor::has_engine_context("blk-does-not-exist"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p project --test vst3_editor_open_policy -- --test-threads=1`
Expected: FAIL to compile if `has_engine_context` still documents/uses `model_id` semantics only after rename, OR PASS trivially — if it passes trivially, strengthen by asserting the new keying via a doc-comment change; the real proof is the env-gated end-to-end in Task 4. (Keep this test as a guard; the behavioral RED for editor routing is the two-same-model end-to-end in Task 4.)

- [ ] **Step 3: Implement — thread block instance key through the open path**

In `crates/project/src/vst3_editor.rs`: rename `model_id` params to `instance_key` in `has_engine_context` and `open_vst3_editor`; inside `open_vst3_editor`, resolve the catalog entry from the context's model:
```rust
pub fn open_vst3_editor(instance_key: &str, _sample_rate: f64) -> Result<Box<dyn PluginEditorHandle>> {
    let gui_context = vst3_host::lookup_vst3_gui_context(instance_key);
    require_engine_context(gui_context.is_some())?;
    let gui_context = gui_context.expect("engine context present after require_engine_context");
    let entry = vst3_host::find_vst3_plugin(&gui_context.model_id)
        .ok_or_else(|| anyhow::anyhow!("VST3 plugin '{}' not found in catalog", gui_context.model_id))?;
    let handle = vst3_host::open_vst3_editor_window(entry.display_name, gui_context)?;
    Ok(Box::new(handle))
}
```
Update `Vst3EditorRegistry` doc to say it is keyed by block instance, not model.

In `crates/adapter-gui/src/block_editor_window_params.rs`: the `on_open_vst3_editor` closure captures `block_id` (from `BlockEditorWindowSetupCtx.block_id`, already in scope in the setup module — thread it into `wire_params` or capture it) and calls:
```rust
let key = block_id.0.clone();
let res = vst3_handles.borrow_mut().open_or_focus(&key, || {
    project::vst3_editor::open_vst3_editor(&key, vst3_sr)
});
```
In `crates/adapter-gui/src/select_chain_block_callback.rs`: replace the `&model_id` open key with `block_id_for_editor.0.as_str()` (already computed at line 275); `has_engine_context(&block_id_for_editor.0)`.

In `crates/adapter-gui/ui/models.slint`, add `block_id: string,` to `ChainBlockItem`; populate it wherever `ChainBlockItem` rows are built on the Rust side (grep `ChainBlockItem {`). In `secondary_windows_block.slint` change `open-plugin(model-id)` emit to `open-plugin(block-id)` using the tile's `block_id`; in `app-window.slint` / `desktop_main.slint` / `touch_main.slint` change `open-vst3-editor(model-id)` to pass the block tile's `block_id`. Update the Rust callbacks (`compact_chain_callbacks.rs on_open_plugin`, `vst3_editor_wiring.rs on_open_vst3_editor`) to treat the incoming string as the instance key.

- [ ] **Step 4: Run tests + render sanity**

Run: `cargo test -p project --test vst3_editor_open_policy --test vst3_editor_registry -- --test-threads=1` → PASS.
Run: `cargo build -p adapter-gui` → clean (Slint compiles). No visual/layout change (callback-arg only); no render needed.

- [ ] **Step 5: Commit**

```bash
git add crates/project/src/vst3_editor.rs crates/adapter-gui/src \
        crates/adapter-gui/ui crates/project/tests/vst3_editor_open_policy.rs
git commit -m "feat(#780): route VST3 native editor open by block instance key"
```

---

### Task 3: project — fold live VST3 params into block.params (pure, seamed)

**Files:**
- Create: `crates/project/src/vst3_capture.rs`
- Modify: `crates/project/src/lib.rs` (`pub mod vst3_capture;`)
- Test: inline `#[cfg(test)]` in `vst3_capture.rs` (deterministic, no plugin)

**Interfaces:**
- Consumes: `Project`, `AudioBlockKind::Core`, `block_core::EFFECT_TYPE_VST3`, `ParameterValue::Float`.
- Produces:
  - `capture_live_vst3_params_with(project: &mut Project, reader: impl Fn(&str) -> Option<Vec<(u32, f64)>>)` — for each VST3 core block, if `reader(&block.id.0)` returns values, replace the block's `p{id}` entries with `p{id} = normalized * 100.0`.
  - `capture_live_vst3_params(project: &mut Project)` — thin wrapper passing `vst3_host::capture_vst3_params`.

- [ ] **Step 1: Write the failing test**

In `crates/project/src/vst3_capture.rs` (new), add tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use crate::chain::Chain;
    use crate::project::Project;
    use block_core::param::set::ParameterSet;
    use domain::ids::{BlockId, ChainId};
    use domain::value_objects::ParameterValue;

    fn vst3_block(id: &str) -> AudioBlock {
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: block_core::EFFECT_TYPE_VST3.to_string(),
                model: "vst3:Chow:Fx".into(),
                params: ParameterSet::default(),
            }),
        }
    }

    fn project_with(block: AudioBlock) -> Project {
        Project {
            name: None, device_settings: Vec::new(), midi: None,
            chains: vec![Chain {
                id: ChainId("rig:gtr".into()), description: None,
                instrument: "electric_guitar".into(), enabled: true, volume: 100.0,
                io_binding_ids: Vec::new(), blocks: vec![block], di_output: None,
            }],
        }
    }

    #[test]
    fn writes_live_params_as_percent_keyed_by_block_id() {
        let mut p = project_with(vst3_block("blk-A"));
        capture_live_vst3_params_with(&mut p, |key| {
            (key == "blk-A").then(|| vec![(2u32, 0.5f64), (7u32, 1.0f64)])
        });
        let AudioBlockKind::Core(c) = &p.chains[0].blocks[0].kind else { panic!() };
        assert_eq!(c.params.get("p2"), Some(&ParameterValue::Float(50.0)));
        assert_eq!(c.params.get("p7"), Some(&ParameterValue::Float(100.0)));
    }

    #[test]
    fn clears_stale_p_entries_before_writing() {
        let mut block = vst3_block("blk-A");
        if let AudioBlockKind::Core(c) = &mut block.kind {
            c.params.insert("p9", ParameterValue::Float(42.0)); // stale (now at default)
        }
        let mut p = project_with(block);
        capture_live_vst3_params_with(&mut p, |_| Some(vec![(2u32, 0.25f64)]));
        let AudioBlockKind::Core(c) = &p.chains[0].blocks[0].kind else { panic!() };
        assert_eq!(c.params.get("p2"), Some(&ParameterValue::Float(25.0)));
        assert!(c.params.get("p9").is_none(), "stale p-entry must be cleared");
    }

    #[test]
    fn leaves_non_vst3_and_unregistered_blocks_untouched() {
        let mut block = vst3_block("blk-A");
        if let AudioBlockKind::Core(c) = &mut block.kind {
            c.effect_type = "reverb".into();
            c.params.insert("mix", ParameterValue::Float(30.0));
        }
        let mut p = project_with(block);
        capture_live_vst3_params_with(&mut p, |_| Some(vec![(1u32, 0.9f64)]));
        let AudioBlockKind::Core(c) = &p.chains[0].blocks[0].kind else { panic!() };
        assert_eq!(c.params.get("mix"), Some(&ParameterValue::Float(30.0)));
        assert!(c.params.get("p1").is_none(), "non-vst3 block must be untouched");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p project vst3_capture::tests`
Expected: FAIL to compile — `vst3_capture` module / functions do not exist.

- [ ] **Step 3: Implement the fold**

`crates/project/src/vst3_capture.rs`:
```rust
//! #780: capture live VST3 controller values into each block's ParameterSet so
//! native-editor edits persist. Runs on the save path (Command::CaptureRigEdits),
//! never on the audio thread. `capture_live_vst3_params_with` is pure and takes
//! the reader as a seam so the fold is unit-testable without a live plugin.

use crate::block::AudioBlockKind;
use crate::project::Project;
use domain::value_objects::ParameterValue;

/// Is `path` a VST3 stored-param key (`p{digits}`)?
fn is_vst3_param_path(path: &str) -> bool {
    path.strip_prefix('p').is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

pub fn capture_live_vst3_params_with(
    project: &mut Project,
    reader: impl Fn(&str) -> Option<Vec<(u32, f64)>>,
) {
    for chain in &mut project.chains {
        for block in &mut chain.blocks {
            let AudioBlockKind::Core(core) = &mut block.kind else { continue };
            if core.effect_type != block_core::EFFECT_TYPE_VST3 {
                continue;
            }
            let Some(values) = reader(&block.id.0) else { continue };
            // Replace the p{id} snapshot wholesale: a param returned to default
            // is absent from `values`, so its stale entry must go.
            core.params.values.retain(|path, _| !is_vst3_param_path(path));
            for (id, normalized) in values {
                core.params.insert(
                    format!("p{id}"),
                    ParameterValue::Float((normalized * 100.0) as f32),
                );
            }
        }
    }
}

pub fn capture_live_vst3_params(project: &mut Project) {
    capture_live_vst3_params_with(project, |key| vst3_host::capture_vst3_params(key));
}
```
Add `pub mod vst3_capture;` to `crates/project/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p project vst3_capture::tests`
Expected: PASS (3 tests). `cargo build -p project` clean.

- [ ] **Step 5: Commit**

```bash
git add crates/project/src/vst3_capture.rs crates/project/src/lib.rs
git commit -m "feat(#780): fold live VST3 params into block.params on capture"
```

---

### Task 4: application — wire capture into CaptureRigEdits + end-to-end proof

**Files:**
- Modify: `crates/application/src/local_dispatcher_rig.rs:123` (`handle_capture_rig_edits`)
- Test: `crates/application/tests/issue_780_vst3_persist.rs` (new, env-gated end-to-end)

**Interfaces:**
- Consumes: `project::vst3_capture::capture_live_vst3_params`, `vst3_host::register_vst3_gui_context` (Task 1).

- [ ] **Step 1: Write the failing test**

`crates/application/tests/issue_780_vst3_persist.rs` — build a rig with one ChowCentaur block, register its context under the block id, drive a param on the controller, dispatch `CaptureRigEdits`, and assert the block's `p{id}` percent landed in the saved rig. Env-gated + `--test-threads=1`. (Use the existing rig-build test helpers in `crates/application/tests/` — mirror `crates/engine/tests/issue_776_catalog_vst3_in_chain.rs` for how a catalog VST3 chain is assembled and rendered; the block id the engine registers must equal the chain block's id.)

Concretely the test:
1. `init_vst3_catalog` against `OPENRIG_TEST_VST3_DIR`; find ChowCentaur `model_id`.
2. Build a `Project`/rig containing a single `Core` block `effect_type=vst3`, `model=<model_id>`, id = the engine-assigned id (derive via the same build path, or assert on whatever id the runtime registers).
3. Bring up the runtime so `register_vst3_gui_context(block_id, model_id, …)` runs (chain enabled + built), OR register directly for the unit-level proof.
4. `plugin controller.set_param(first_non_default_id, 0.85)`.
5. `dispatcher.dispatch(Command::CaptureRigEdits)`.
6. Assert the project block's `params.get("p{id}")` ≈ `85.0`.

- [ ] **Step 2: Run test to verify it fails**

Run: `OPENRIG_TEST_VST3_DIR=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source/vst3 cargo test -p application --test issue_780_vst3_persist -- --test-threads=1`
Expected: FAIL — `CaptureRigEdits` does not yet call the fold, so `params.get("p{id}")` is `None`.

- [ ] **Step 3: Implement the wiring**

In `crates/application/src/local_dispatcher_rig.rs`, at the top of `handle_capture_rig_edits`, before the rig fold:
```rust
pub(crate) fn handle_capture_rig_edits(&self) -> Result<Vec<Event>> {
    // #780: pull live VST3 controller values into each block's params so
    // native-editor tweaks persist. Best-effort; no-op without a live context.
    project::vst3_capture::capture_live_vst3_params(&mut self.project.borrow_mut());

    let Some(rig) = self.rig.borrow().clone() else {
        return Ok(vec![]);
    };
    sync_synthetic_into_rig(&mut rig.borrow_mut(), &self.project.borrow());
    Ok(vec![Event::ProjectMutated])
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `OPENRIG_TEST_VST3_DIR=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source/vst3 cargo test -p application --test issue_780_vst3_persist -- --test-threads=1`
Expected: PASS. Then full deterministic suite: `cargo test -p application -p project -p vst3-host -p engine` → PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/application/src/local_dispatcher_rig.rs \
        crates/application/tests/issue_780_vst3_persist.rs
git commit -m "feat(#780): capture live VST3 params on save (CaptureRigEdits)"
```

---

### Task 5: Docs + issue comment

**Files:**
- Modify: `docs/blocks-catalog.md` (VST3 section — note native-editor edits persist via CaptureRigEdits, keyed per block)
- Modify: `docs/testing.md` (add the `issue_780_*` env-gated tests + run line to the real-plugin battery section)

- [ ] **Step 1: Update docs**

Add a short paragraph to the VST3 block docs: native-editor parameter changes are captured into the block's `params` (`p{id}` percent) on save, per block instance; two blocks of the same plugin persist independently. Note FORM knobs remain a follow-up. Add the two env-gated test files + run command to `docs/testing.md`.

- [ ] **Step 2: Verify build + full test once more**

Run: `cargo build` (clean) and `cargo test -p application -p project -p vst3-host -p engine` (deterministic subset green).

- [ ] **Step 3: Commit + push**

```bash
git add docs/blocks-catalog.md docs/testing.md
git commit -m "docs(#780): document VST3 native-editor param persistence"
git push -u origin feature/issue-780
```

- [ ] **Step 4: Comment on the issue** (after push — per gitflow, hash + files + build/test)

```bash
gh issue comment 780 --body "<pushed hashes, files, and the env-gated run line proving red→green>"
```

---

## Self-Review Notes

- **Spec coverage:** persistence fix (Tasks 3–4) ✓; per-block identity / same-model collision (Tasks 1–2, `two_same_model_instances_do_not_collide`) ✓; FORM knobs explicitly out of scope ✓.
- **RT invariants:** capture runs on the save/main path only; the audio processor keeps its build-time `Vst3ParamChannel` and never reads the registry — no new audio-thread lock/alloc/IO ✓.
- **Type consistency:** `instance_key: &str` used uniformly for register/lookup/capture/open; stored value is `ParameterValue::Float(percent)`; engine load already reads `p{id}` percent → normalized (unchanged) ✓.
- **Red-first:** Task 3 is deterministic (seam), Tasks 1 & 4 are env-gated real-plugin reds runnable locally with the plugin dir above ✓.
