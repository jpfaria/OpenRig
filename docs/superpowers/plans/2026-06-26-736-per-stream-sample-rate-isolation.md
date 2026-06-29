# Per-stream sample-rate isolation (#736) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow two I/O bindings at different device sample rates (e.g. Scarlett 44.1 kHz + TEYUN 48 kHz) to run simultaneously in one chain, each isolated stream clocked at its own device's rate.

**Architecture:** The cpal streams already open at each device's native rate (`stream_builder.rs` uses `resolved_input_sample_rate(device)` per device — #669). Two things still force one clock per chain: (1) `resolve_multi_io_sample_rate()` `bail!`s when rates differ anywhere in the chain, and (2) every per-input runtime is built at one shared `sample_rate`. The fix resolves the rate **per binding-group** (keeping the "input rate == output rate *within* a binding" check, dropping the cross-binding check) and threads a per-input-device rate map into the per-input runtime build so each isolated `ChainRuntimeState` is clocked at its own device's rate. Single-binding chains resolve to one group → one rate → byte-identical to today.

**Tech Stack:** Rust workspace (`engine`, `infra-cpal`, `domain`, `project` crates). cpal audio backend (non-JACK path; JACK is cfg-guarded and out of scope).

## Global Constraints

- **No regression to single-binding chains** — one binding → one group → one rate → bit-identical audio path. `golden`, `volume_invariants`, `stream_isolation` (and `stream_isolation_same_device`) tests MUST stay green.
- **Within a single binding, input rate must still equal output rate** — one isolated stream needs no internal resample. Only *cross-binding* rate differences become allowed.
- **cpal path first.** JACK path (`cfg(all(target_os = "linux", feature = "jack"))`) is separate and unchanged.
- The full multi-rate path is **hardware-validated** (two interfaces at different rates) and belongs in the `OPENRIG_HW_TESTS=1` battery (#670). It CANNOT be proven headless — only the pure rate logic and the runtime-clock wiring are unit-testable. The implementer must NOT claim the hardware path works without the owner running the gated test.
- Engine sample-rate type is `f32`; device-resolved rates are `u32` (`resolved_input_sample_rate` → `u32`), cast `as f32` at the boundary, exactly as `unify_io_sample_rates` does today.
- Key type for the per-device rate map is `domain::ids::DeviceId` (already used by both `engine` and `infra-cpal`).

---

## File Structure

- `crates/infra-cpal/src/stream_config.rs` — ADD pure `resolve_binding_sample_rates()` (per-binding validation + per-device map). Keep `unify_io_sample_rates` (reused per binding).
- `crates/infra-cpal/src/resolved.rs` — ADD `by_device: HashMap<DeviceId, f32>` field to `ResolvedChainAudioConfig`.
- `crates/infra-cpal/src/chain_resolve.rs` — `resolve_chain_audio_config` and `resolve_project_chain_sample_rates` use the new per-binding resolver (stop cross-binding `bail!`).
- `crates/infra-cpal/src/build_request.rs` — ADD `device_sample_rates: HashMap<DeviceId, f32>` field to `BuildRequest`; pass it through `build_chain_runtime`.
- `crates/infra-cpal/src/controller.rs` — populate the map from `resolved.by_device` in `schedule_chain_activation` and `upsert_chain_with_resolved`.
- `crates/engine/src/runtime_graph.rs` — ADD `device_rates: &HashMap<DeviceId, f32>` param to `build_per_input_runtimes`, `build_per_input_runtime_states`, `RuntimeGraph::upsert_chain[_spillover]`, `upsert_chain_impl`; per-group rate resolution; `build_runtime_graph` passes empty map; `update_chain_runtime_state_impl` uses `runtime.sample_rate()` for the in-place rebuild rate.
- `crates/engine/src/rig_runtime.rs` — the 4 `graph.upsert_chain(...)` calls pass an empty map.

---

### Task 1: Pure per-binding rate resolver (headless-testable core)

**Files:**
- Modify: `crates/infra-cpal/src/stream_config.rs` (add fn near `unify_io_sample_rates`, ~line 215)
- Test: same file, `unify_rate_tests` module (extend)

**Interfaces:**
- Consumes: `unify_io_sample_rates(chain_id, &[u32], &[u32]) -> Result<f32>` (existing).
- Produces: `resolve_binding_sample_rates(chain_id: &str, bindings: &[(Vec<u32>, Vec<u32>)]) -> Result<f32>` — each tuple is one binding's `(input_rates, output_rates)`. Validates input==output **within** each binding (reusing `unify_io_sample_rates`), **allows** cross-binding differences, returns the FIRST binding's rate as the chain's representative rate. Errors only on within-binding mismatch or no I/O at all.

- [ ] **Step 1: Write the failing tests**

Add to the `unify_rate_tests` module in `crates/infra-cpal/src/stream_config.rs`:

```rust
    use super::resolve_binding_sample_rates;

    #[test]
    fn two_bindings_at_different_rates_resolve_without_error() {
        // Scarlett binding @44.1k, TEYUN binding @48k — the #736 case.
        // Cross-binding difference is allowed; representative = first binding.
        let rate = resolve_binding_sample_rates(
            "c",
            &[(vec![44_100], vec![44_100]), (vec![48_000], vec![48_000])],
        )
        .expect("cross-binding rate difference must be allowed");
        assert_eq!(rate, 44_100.0);
    }

    #[test]
    fn within_binding_input_output_mismatch_still_errors() {
        // 44.1k input + 48k output INSIDE one binding — one isolated stream
        // cannot internally resample, so this stays a loud error (#669 shape).
        let e = resolve_binding_sample_rates("c", &[(vec![44_100], vec![48_000])])
            .unwrap_err()
            .to_string();
        assert!(e.contains("across I/O"), "got: {e}");
    }

    #[test]
    fn single_binding_matches_legacy_unify() {
        // One binding with all the chain's I/O → identical to whole-chain unify.
        let rate = resolve_binding_sample_rates(
            "c",
            &[(vec![44_100, 44_100], vec![44_100])],
        )
        .unwrap();
        assert_eq!(rate, 44_100.0);
    }

    #[test]
    fn no_bindings_is_an_error() {
        assert!(resolve_binding_sample_rates("c", &[]).is_err());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p infra-cpal --lib unify_rate_tests`
Expected: FAIL — `cannot find function resolve_binding_sample_rates`.

- [ ] **Step 3: Implement the function**

Add after `unify_io_sample_rates` (after line 215) in `crates/infra-cpal/src/stream_config.rs`:

```rust
/// Resolve the sample rate **per binding-group** instead of once for the whole
/// chain (#736). Each `(input_rates, output_rates)` tuple is one I/O binding's
/// device rates. Within a binding, every input and output must agree (one
/// isolated stream needs no internal resample) — reuses `unify_io_sample_rates`,
/// so the within-binding error wording is unchanged ("across inputs" / "across
/// I/O"). Across bindings, rates may DIFFER freely — that is the whole point of
/// invariant #4 (stream isolation): two isolated streams share no clock.
///
/// Returns the FIRST binding's rate as the chain's representative scalar (used
/// for legacy single-rate consumers: stream-signature back-compat, DI-loop
/// resample target). The authoritative per-device rates flow separately through
/// `ResolvedChainAudioConfig::by_device`. With a single binding this equals the
/// legacy whole-chain `unify_io_sample_rates` result — single-binding chains are
/// bit-identical.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_binding_sample_rates(
    chain_id: &str,
    bindings: &[(Vec<u32>, Vec<u32>)],
) -> Result<f32> {
    let mut representative: Option<f32> = None;
    for (input_rates, output_rates) in bindings {
        let rate = unify_io_sample_rates(chain_id, input_rates, output_rates)?;
        if representative.is_none() {
            representative = Some(rate);
        }
    }
    representative.ok_or_else(|| anyhow!("chain '{}' has no inputs or outputs", chain_id))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p infra-cpal --lib unify_rate_tests`
Expected: PASS (all old `unify_io_sample_rates` tests + the 4 new ones).

- [ ] **Step 5: Commit**

```bash
git add crates/infra-cpal/src/stream_config.rs
git commit -m "feat(#736): resolve_binding_sample_rates — per-binding rate, cross-binding allowed"
```

---

### Task 2: Resolve layer produces a per-device rate map

**Files:**
- Modify: `crates/infra-cpal/src/resolved.rs:120-126` (add `by_device` field)
- Modify: `crates/infra-cpal/src/chain_resolve.rs` (`resolve_chain_audio_config` ~362-383, `resolve_project_chain_sample_rates` ~90-107)

**Interfaces:**
- Consumes: `resolve_binding_sample_rates` (Task 1); `engine::runtime_endpoints::{resolve_chain_io, resolve_chain_io_by_binding, BindingIo}`; `resolved_input_sample_rate`/`resolved_output_sample_rate`.
- Produces: `ResolvedChainAudioConfig { …, sample_rate: f32, by_device: HashMap<DeviceId, f32>, … }`. `by_device` maps every resolved input/output `DeviceId` to its resolved rate (`as f32`).

- [ ] **Step 1: Add the `by_device` field**

In `crates/infra-cpal/src/resolved.rs`, change the struct (lines 120-126) to:

```rust
#[allow(dead_code)]
pub(crate) struct ResolvedChainAudioConfig {
    pub(crate) inputs: Vec<ResolvedInputDevice>,
    pub(crate) outputs: Vec<ResolvedOutputDevice>,
    pub(crate) sample_rate: f32,
    /// Per-input-device resolved rate (#736). One isolated runtime per input
    /// device is clocked at its own rate from this map; the scalar
    /// `sample_rate` above is only the representative (first binding) rate for
    /// legacy single-rate consumers.
    pub(crate) by_device: std::collections::HashMap<domain::ids::DeviceId, f32>,
    pub(crate) stream_signature: ChainStreamSignature,
}
```

- [ ] **Step 2: Build the map and per-binding rate in `resolve_chain_audio_config`**

In `crates/infra-cpal/src/chain_resolve.rs`, replace the body of `resolve_chain_audio_config` (lines 362-383). Add near the top of the file the imports it needs (verify they are not already present):

```rust
use std::collections::HashMap;
use domain::ids::DeviceId;
use engine::runtime_endpoints::{resolve_chain_io, resolve_chain_io_by_binding};
```

New body:

```rust
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_chain_audio_config(
    host: &cpal::Host,
    project: &Project,
    chain: &Chain,
    registry: &[IoBinding],
) -> Result<ResolvedChainAudioConfig> {
    let inputs = resolve_chain_inputs(host, project, chain, registry)?;
    let outputs = resolve_chain_outputs(host, project, chain, registry)?;

    // #736: map each resolved device to its own rate. The resolved input /
    // output lists are in the same order as the logical endpoints from
    // `resolve_chain_io`, so we zip to recover each device id.
    let (logical_inputs, logical_outputs) = resolve_chain_io(chain, registry);
    let mut by_device: HashMap<DeviceId, f32> = HashMap::new();
    for (logical, resolved) in logical_inputs.iter().zip(inputs.iter()) {
        by_device.insert(
            logical.device_id.clone(),
            crate::resolved_input_sample_rate(resolved) as f32,
        );
    }
    for (logical, resolved) in logical_outputs.iter().zip(outputs.iter()) {
        by_device.insert(
            logical.device_id.clone(),
            crate::resolved_output_sample_rate(resolved) as f32,
        );
    }

    // #736: validate per binding (input==output within a binding) and allow
    // different rates across bindings, instead of one whole-chain unify.
    let binding_rates: Vec<(Vec<u32>, Vec<u32>)> = resolve_chain_io_by_binding(chain, registry)
        .iter()
        .map(|g| {
            let in_r = g
                .inputs
                .iter()
                .map(|e| by_device.get(&e.device_id).copied().unwrap_or(0.0) as u32)
                .collect();
            let out_r = g
                .outputs
                .iter()
                .map(|e| by_device.get(&e.device_id).copied().unwrap_or(0.0) as u32)
                .collect();
            (in_r, out_r)
        })
        .collect();
    let sample_rate = crate::resolve_binding_sample_rates(&chain.id.0, &binding_rates)?;

    let stream_signature: ChainStreamSignature =
        crate::build_chain_stream_signature_multi(chain, &inputs, &outputs, registry);

    Ok(ResolvedChainAudioConfig {
        inputs,
        outputs,
        sample_rate,
        by_device,
        stream_signature,
    })
}
```

- [ ] **Step 3: Stop the cross-binding `bail!` in `resolve_project_chain_sample_rates`**

In `crates/infra-cpal/src/chain_resolve.rs`, the non-JACK block (lines 90-107) builds `HashMap<ChainId, f32>`. Replace the per-chain `resolve_multi_io_sample_rate(...)` line with the per-binding resolver so the console/offline path no longer rejects cross-binding chains (it stores the representative rate; per-device runtime rates on that path are a follow-up):

```rust
        for chain in &project.chains {
            if !chain.enabled {
                continue;
            }
            let inputs = resolve_chain_inputs(&host, project, chain, registry)?;
            let outputs = resolve_chain_outputs(&host, project, chain, registry)?;
            let (logical_inputs, logical_outputs) =
                engine::runtime_endpoints::resolve_chain_io(chain, registry);
            let mut by_device: std::collections::HashMap<domain::ids::DeviceId, u32> =
                std::collections::HashMap::new();
            for (logical, resolved) in logical_inputs.iter().zip(inputs.iter()) {
                by_device.insert(logical.device_id.clone(), crate::resolved_input_sample_rate(resolved));
            }
            for (logical, resolved) in logical_outputs.iter().zip(outputs.iter()) {
                by_device.insert(logical.device_id.clone(), crate::resolved_output_sample_rate(resolved));
            }
            let binding_rates: Vec<(Vec<u32>, Vec<u32>)> =
                engine::runtime_endpoints::resolve_chain_io_by_binding(chain, registry)
                    .iter()
                    .map(|g| {
                        let in_r = g.inputs.iter().map(|e| by_device.get(&e.device_id).copied().unwrap_or(0)).collect();
                        let out_r = g.outputs.iter().map(|e| by_device.get(&e.device_id).copied().unwrap_or(0)).collect();
                        (in_r, out_r)
                    })
                    .collect();
            let sample_rate = crate::resolve_binding_sample_rates(&chain.id.0, &binding_rates)?;
            sample_rates.insert(chain.id.clone(), sample_rate);
        }
```

- [ ] **Step 4: Compile + existing infra tests green**

Run: `cargo test -p infra-cpal --lib`
Expected: PASS. Build error here usually means `by_device` not set at a second `ResolvedChainAudioConfig { … }` construction site — grep `ResolvedChainAudioConfig {` and add `by_device` to any other.

Run: `cargo build -p infra-cpal`
Expected: clean (a now-unused `resolve_multi_io_sample_rate` may warn; leave it — its `unify` tests still cover the core, and removal is deferred to avoid touching unrelated tests).

- [ ] **Step 5: Commit**

```bash
git add crates/infra-cpal/src/resolved.rs crates/infra-cpal/src/chain_resolve.rs
git commit -m "feat(#736): resolve layer emits per-device rate map; per-binding validation"
```

---

### Task 3: Clock each per-input runtime at its own device's rate

**Files:**
- Modify: `crates/engine/src/runtime_graph.rs` (`build_per_input_runtimes` 177-215, `build_per_input_runtime_states` 227-239, `build_runtime_graph` 147-171, import line 45)
- Test: `crates/engine/src/runtime_graph.rs` — new `#[cfg(test)]` module at end of file

**Interfaces:**
- Consumes: `domain::ids::DeviceId`; `ChainSegment.input.device_id` (existing field).
- Produces: `build_per_input_runtimes(chain, sample_rate, device_rates: &HashMap<DeviceId, f32>, elastic_targets, registry)` and `build_per_input_runtime_states(chain, sample_rate, device_rates: &HashMap<DeviceId, f32>, elastic_targets, registry)`. For each group, the runtime is built at `device_rates.get(group_input_device).copied().unwrap_or(sample_rate)`. **Empty map → every group uses `sample_rate` → bit-identical.**

- [ ] **Step 1: Write the failing test**

Append to `crates/engine/src/runtime_graph.rs`:

```rust
#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]
mod issue_736_per_binding_rate_tests {
    use super::build_per_input_runtimes;
    use std::collections::HashMap;
    use domain::ids::DeviceId;
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use project::chain::Chain;

    // Two mono inputs on two devices, paired with two outputs on the same two
    // devices — two bindings, the #736 "Scarlett + TEYUN" shape.
    fn two_binding_registry() -> Vec<IoBinding> {
        vec![
            IoBinding {
                id: "a".into(),
                name: "A".into(),
                inputs: vec![IoEndpoint { name: "in".into(), device_id: DeviceId("devA".into()), mode: ChannelMode::Mono, channels: vec![0] }],
                outputs: vec![IoEndpoint { name: "out".into(), device_id: DeviceId("devA".into()), mode: ChannelMode::Stereo, channels: vec![0, 1] }],
            },
            IoBinding {
                id: "b".into(),
                name: "B".into(),
                inputs: vec![IoEndpoint { name: "in".into(), device_id: DeviceId("devB".into()), mode: ChannelMode::Mono, channels: vec![0] }],
                outputs: vec![IoEndpoint { name: "out".into(), device_id: DeviceId("devB".into()), mode: ChannelMode::Stereo, channels: vec![0, 1] }],
            },
        ]
    }

    fn two_binding_chain() -> Chain {
        let mut c = Chain::new_for_test("c");
        c.io_binding_ids = vec!["a".into(), "b".into()];
        c
    }

    #[test]
    fn each_runtime_built_at_its_own_device_rate() {
        let chain = two_binding_chain();
        let registry = two_binding_registry();
        let mut rates = HashMap::new();
        rates.insert(DeviceId("devA".into()), 44_100.0_f32);
        rates.insert(DeviceId("devB".into()), 48_000.0_f32);

        let runtimes = build_per_input_runtimes(&chain, 48_000.0, &rates, &[], &registry)
            .expect("two-binding build must succeed");
        assert_eq!(runtimes.len(), 2, "two devices → two isolated runtimes");

        let mut seen: Vec<f32> = runtimes.iter().map(|(_, s)| s.sample_rate()).collect();
        seen.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(seen, vec![44_100.0, 48_000.0], "each runtime clocked at its own device rate");
    }

    #[test]
    fn empty_rate_map_falls_back_to_scalar_bit_exact() {
        let chain = two_binding_chain();
        let registry = two_binding_registry();
        let empty: HashMap<DeviceId, f32> = HashMap::new();
        let runtimes = build_per_input_runtimes(&chain, 48_000.0, &empty, &[], &registry).unwrap();
        for (_, state) in &runtimes {
            assert_eq!(state.sample_rate(), 48_000.0, "no override → scalar rate (legacy)");
        }
    }
}
```

> Note: if `Chain::new_for_test` / `io_binding_ids` field names differ, mirror the construction used in `crates/engine/tests/issue_716_per_binding_routing.rs` (the existing two-binding test) — copy its chain+registry fixture verbatim rather than inventing one.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p engine --lib issue_736_per_binding_rate_tests`
Expected: FAIL — `build_per_input_runtimes` takes 4 args, not 5 (arity mismatch).

- [ ] **Step 3: Thread the per-device rate map**

In `crates/engine/src/runtime_graph.rs`:

(a) Add `DeviceId` to the domain import on line 45:
```rust
use domain::ids::{BlockId, ChainId, DeviceId};
```

(b) Change `build_per_input_runtimes` (signature + the per-group loop, lines 177-215):
```rust
pub(crate) fn build_per_input_runtimes(
    chain: &Chain,
    sample_rate: f32,
    device_rates: &HashMap<DeviceId, f32>,
    elastic_targets: &[usize],
    registry: &[IoBinding],
) -> Result<Vec<(usize, ChainRuntimeState)>> {
    let (resolved_inputs, resolved_outputs) = resolve_chain_io(chain, registry);
    let (eff_inputs, eff_input_cpal_indices, eff_split_positions, eff_entry_groups) =
        effective_inputs(chain, &resolved_inputs, registry);
    let eff_outputs = effective_outputs(chain, &resolved_outputs, registry);
    let all_segments = split_chain_into_segments(
        chain,
        &eff_inputs,
        &eff_input_cpal_indices,
        &eff_split_positions,
        &eff_entry_groups,
        &eff_outputs,
        registry,
    );
    let groups = group_segments_by_input(chain, all_segments);
    let mut out = Vec::with_capacity(groups.len());
    for (group, segments) in groups {
        let cpal_input_index = segments.first().map(|s| s.cpal_input_index).unwrap_or(0);
        // #736: this group is one isolated input → one device → its OWN rate.
        // Empty/absent override falls back to the chain scalar (bit-identical
        // single-binding behaviour). Within a binding, input rate == output
        // rate is validated at resolve time, so the input device's rate is the
        // whole stream's rate.
        let group_rate = segments
            .first()
            .and_then(|s| device_rates.get(&s.input.device_id).copied())
            .unwrap_or(sample_rate);
        let mut state = assemble_chain_runtime_state(
            chain,
            &segments,
            &eff_outputs,
            group_rate,
            elastic_targets,
            None,
        )?;
        state.owned_entry = Some((group, cpal_input_index));
        out.push((group, state));
    }
    Ok(out)
}
```

(c) Change `build_per_input_runtime_states` (227-239):
```rust
pub fn build_per_input_runtime_states(
    chain: &Chain,
    sample_rate: f32,
    device_rates: &HashMap<DeviceId, f32>,
    elastic_targets: &[usize],
    registry: &[IoBinding],
) -> Result<Vec<(usize, Arc<ChainRuntimeState>)>> {
    Ok(
        build_per_input_runtimes(chain, sample_rate, device_rates, elastic_targets, registry)?
            .into_iter()
            .map(|(group, state)| (group, Arc::new(state)))
            .collect(),
    )
}
```

(d) Update the `build_runtime_graph` call site (line 165) to pass an empty map (offline/rig path stays single-rate-per-chain — bit-identical):
```rust
        let no_device_rates: HashMap<DeviceId, f32> = HashMap::new();
        for (group, state) in
            build_per_input_runtimes(chain, sample_rate, &no_device_rates, elastic_targets, registry)?
        {
            state.set_volume_pct(chain.volume);
            chains.insert((chain.id.clone(), group), Arc::new(state));
        }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p engine --lib issue_736_per_binding_rate_tests`
Expected: PASS — two runtimes at 44.1k/48k; empty map → both 48k.

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/runtime_graph.rs
git commit -m "feat(#736): build_per_input_runtimes clocks each runtime at its device rate"
```

---

### Task 4: Live-rebuild paths respect per-runtime rate

**Files:**
- Modify: `crates/engine/src/runtime_graph.rs` (`update_chain_runtime_state_impl` ~686-945, `RuntimeGraph::upsert_chain[_spillover]` 977-1013, `upsert_chain_impl` 1015-1105)
- Test: `crates/engine/src/runtime_graph.rs` — extend the Task-3 test module

**Interfaces:**
- `update_chain_runtime_state_impl` keeps its `sample_rate: f32` param but, for the per-input rebuild, uses `runtime.sample_rate()` (the rate the runtime was actually built at) so an in-place param edit on the 44.1k runtime rebuilds its blocks at 44.1k, not the chain scalar. For single-binding `runtime.sample_rate() == sample_rate` → bit-identical.
- `RuntimeGraph::upsert_chain[_spillover]` / `upsert_chain_impl` gain `device_rates: &HashMap<DeviceId, f32>`, forwarded to `build_per_input_runtimes` in the full-rebuild branch.

- [ ] **Step 1: Write the failing test**

Add to the `issue_736_per_binding_rate_tests` module:

```rust
    use super::RuntimeGraph;

    #[test]
    fn upsert_full_rebuild_preserves_per_device_rates() {
        let chain = two_binding_chain();
        let registry = two_binding_registry();
        let mut rates = HashMap::new();
        rates.insert(DeviceId("devA".into()), 44_100.0_f32);
        rates.insert(DeviceId("devB".into()), 48_000.0_f32);

        let mut graph = RuntimeGraph { chains: std::collections::HashMap::new() };
        graph
            .upsert_chain(&chain, 48_000.0, &rates, false, &[], &registry)
            .expect("initial upsert");
        let mut seen: Vec<f32> = graph
            .runtimes_for(&chain.id)
            .iter()
            .map(|r| r.sample_rate())
            .collect();
        seen.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(seen, vec![44_100.0, 48_000.0]);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p engine --lib issue_736_per_binding_rate_tests::upsert_full_rebuild_preserves_per_device_rates`
Expected: FAIL — `upsert_chain` takes 5 args, not 6 (arity mismatch).

- [ ] **Step 3: Thread the map through upsert; switch in-place rebuild to `runtime.sample_rate()`**

In `crates/engine/src/runtime_graph.rs`:

(a) `update_chain_runtime_state_impl` (around lines 766 and 772 — the two `build_input_processing_state(..., sample_rate, ...)` calls): replace the `sample_rate` argument passed to `build_input_processing_state` with `runtime.sample_rate()`. (The function param `sample_rate` stays in the signature; it is now only the legacy default and is not used for the rebuild rate. Add a one-line comment: `// #736: rebuild at the runtime's OWN built rate, not the chain scalar`.)

(b) `RuntimeGraph::upsert_chain` (977-993), `upsert_chain_spillover` (997-1013), and `upsert_chain_impl` (1016-1024): add `device_rates: &HashMap<DeviceId, f32>` as the parameter immediately after `sample_rate`, and forward it. The two `upsert_chain*` public methods forward `device_rates` into `self.upsert_chain_impl(chain, sample_rate, device_rates, …)`.

(c) In `upsert_chain_impl`, the full-rebuild branch (line 1096):
```rust
        for (group, state) in
            build_per_input_runtimes(chain, sample_rate, device_rates, elastic_targets, registry)?
        {
```
The in-place branch (lines 1056-1078) is unchanged — it calls `update_chain_runtime_state[_spillover]` which now reads `runtime.sample_rate()` internally, so it needs no map.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p engine --lib issue_736_per_binding_rate_tests`
Expected: PASS (all three tests).

- [ ] **Step 5: Fix engine-internal + rig callers, then full engine test**

`rig_runtime.rs` has 4 `graph.upsert_chain(&chain, …)` calls (lines ~316, 358, 411, 445). Insert `&HashMap::new()` after the sample-rate arg in each (the rig path stays single-rate). Add `use std::collections::HashMap;` if absent.

Run: `cargo test -p engine`
Expected: PASS — `volume_invariants`, `stream_isolation`, `stream_isolation_same_device`, `issue_716_per_binding_routing`, golden, and the new #736 tests all green.

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/runtime_graph.rs crates/engine/src/rig_runtime.rs
git commit -m "feat(#736): upsert + in-place rebuild honor each runtime's own rate"
```

---

### Task 5: Carry the per-device map through the cpal build seam

**Files:**
- Modify: `crates/infra-cpal/src/build_request.rs` (add field 19-30; pass map 41-48)
- Modify: `crates/infra-cpal/src/controller.rs` (`schedule_chain_activation` ~821, `upsert_chain_with_resolved` ~1010-1025)

**Interfaces:**
- `BuildRequest { …, device_sample_rates: HashMap<DeviceId, f32> }`.
- `build_chain_runtime` passes `&req.device_sample_rates` to `build_per_input_runtime_states`.
- `schedule_chain_activation` sets `device_sample_rates: resolved.by_device`.
- `upsert_chain_with_resolved` passes `&resolved.by_device` to `upsert_chain[_spillover]`.

- [ ] **Step 1: Add the field and thread it in `build_request.rs`**

```rust
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use domain::io_binding::IoBinding;
use domain::ids::DeviceId;
use engine::runtime::{build_per_input_runtime_states, ChainRuntimeState};
use project::chain::Chain;

pub struct BuildRequest {
    pub chain: Chain,
    /// Representative / fallback rate (Hz) — first binding's rate (#736).
    pub sample_rate: f32,
    /// Per-input-device rates (#736). Each isolated runtime is clocked at its
    /// own device's rate; missing device falls back to `sample_rate`.
    pub device_sample_rates: HashMap<DeviceId, f32>,
    pub buffer_sizes: Vec<usize>,
    pub io_bindings: Vec<IoBinding>,
}

pub fn build_chain_runtime(req: &BuildRequest) -> Result<Vec<(usize, Arc<ChainRuntimeState>)>> {
    build_per_input_runtime_states(
        &req.chain,
        req.sample_rate,
        &req.device_sample_rates,
        &req.buffer_sizes,
        &req.io_bindings,
    )
}
```

- [ ] **Step 2: Populate the map in `schedule_chain_activation`**

In `crates/infra-cpal/src/controller.rs`, the `BuildRequest { … }` construction (~line 821) becomes:
```rust
            let request = BuildRequest {
                chain: chain_for_build,
                sample_rate: resolved.sample_rate,
                device_sample_rates: resolved.by_device.clone(),
                buffer_sizes: elastic_targets,
                io_bindings: registry_for_build,
            };
```

- [ ] **Step 3: Pass the map in `upsert_chain_with_resolved`**

In `crates/infra-cpal/src/controller.rs` (lines ~1010-1025), add `&resolved.by_device` after the `resolved.sample_rate` arg in BOTH `upsert_chain_spillover` and `upsert_chain`:
```rust
        if spillover {
            self.runtime_graph.upsert_chain_spillover(
                chain,
                resolved.sample_rate,
                &resolved.by_device,
                needs_stream_rebuild,
                &elastic_targets,
                &self.io_bindings,
            )?;
        } else {
            self.runtime_graph.upsert_chain(
                chain,
                resolved.sample_rate,
                &resolved.by_device,
                needs_stream_rebuild,
                &elastic_targets,
                &self.io_bindings,
            )?;
        }
```

- [ ] **Step 4: Fix infra-cpal test constructions of `BuildRequest`, then build + test**

Grep `BuildRequest {` across `crates/infra-cpal` (e.g. `controller_taps.rs`, `tests/build_chain_runtime.rs`) and add `device_sample_rates: std::collections::HashMap::new(),` to each (tests use the scalar-fallback path).

Run: `cargo test -p infra-cpal`
Expected: PASS — controller / slot / regression tests green; `device_sample_rates` empty in tests → scalar fallback → unchanged.

- [ ] **Step 5: Commit**

```bash
git add crates/infra-cpal/src/build_request.rs crates/infra-cpal/src/controller.rs crates/infra-cpal/src/controller_taps.rs crates/infra-cpal/tests/build_chain_runtime.rs
git commit -m "feat(#736): thread per-device rates through BuildRequest + controller"
```

---

### Task 6: Workspace build + headless regression sweep

**Files:** none (verification task).

- [ ] **Step 1: Whole workspace builds**

Run: `cargo build --workspace`
Expected: clean. Fix any remaining call sites (adapter-console / adapter-console-rig pass `&HashMap::new()` if they call any changed engine fn directly — they call `build_runtime_graph`, whose signature is unchanged, so likely nothing).

- [ ] **Step 2: Full headless test suite (the golden gate)**

Run: `cargo test --workspace`
Expected: PASS. Specifically confirm green: `volume_invariants`, `stream_isolation`, `stream_isolation_same_device`, `issue_716_per_binding_routing`, golden render tests, `unify_rate_tests`, `issue_736_per_binding_rate_tests`.

- [ ] **Step 3: Clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean. If `resolve_multi_io_sample_rate` is now dead and clippy/`dead_code` complains, either delete it (and its now-redundant `resolve_multi_io_*` callers/tests) or `#[allow(dead_code)]` with a `// superseded by resolve_binding_sample_rates (#736)` note. Prefer deletion if no test references it.

- [ ] **Step 4: Commit any clippy fixes**

```bash
git add -A && git commit -m "chore(#736): clippy + remove superseded resolve_multi_io_sample_rate"
```

---

### Task 7: Hardware-gated multi-rate test (owner-run, NOT headless)

**Files:**
- Create or extend: `crates/infra-cpal/tests/issue_736_multi_rate_streams.rs` (gated behind `OPENRIG_HW_TESTS=1`, mirroring `issue_670_real_streams_no_xruns.rs`)

**Interfaces:** mirrors the existing #670 hardware battery harness — discover two real interfaces, build a two-binding chain at different rates, activate, run audio, assert zero xruns and that both runtimes report their own rate.

- [ ] **Step 1: Author the gated test**

Mirror `crates/infra-cpal/tests/issue_670_real_streams_no_xruns.rs`: skip with a log line unless `std::env::var("OPENRIG_HW_TESTS").as_deref() == Ok("1")`. With two interfaces present, build a chain with two bindings at 44.1 kHz and 48 kHz, activate both streams, run ~10 s, and assert: (a) activation succeeds with no "mismatched sample rates" error, (b) `graph.runtimes_for(chain)` reports rates `{44100, 48000}`, (c) `xrun_count` stays 0 on both runtimes. Copy the device-discovery + activation scaffolding from the #670 file verbatim; do not invent a new harness.

- [ ] **Step 2: Confirm it compiles and is correctly skipped headless**

Run: `cargo test -p infra-cpal --test issue_736_multi_rate_streams`
Expected: the test compiles and PASSES-by-skipping (logs "skipped: set OPENRIG_HW_TESTS=1") on this CI/dev machine without hardware.

- [ ] **Step 3: Commit; flag the owner-run step**

```bash
git add crates/infra-cpal/tests/issue_736_multi_rate_streams.rs
git commit -m "test(#736): hardware-gated two-interface multi-rate battery (OPENRIG_HW_TESTS=1)"
```

> **Owner action (cannot be done in CI/headless):** with a Scarlett @44.1 kHz and a TEYUN @48 kHz both connected, run
> `OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release --test issue_736_multi_rate_streams`
> and confirm both streams activate with zero xruns/cross-talk. This is the acceptance evidence for #736; the headless suite cannot produce it.

---

## Acceptance (from the issue)

- A chain with two bindings at different device rates activates **both** streams, each at its own rate, zero xruns/cross-talk → **Task 7 (owner-run)**.
- No "mismatched sample rates" error for cross-binding differences → **Tasks 1–2** (validation moved per-binding).
- Single-binding chains unchanged (golden bit-exact) → **Tasks 3, 4, 6** (empty/uniform map → scalar fallback; full headless suite green).

## Self-Review notes

- Spec fix-shape coverage: (1) per-binding rate resolution → Task 1+2; (2) per-group rate threaded into build → Task 3; (3) per-binding validation replacing cross-input → Task 1+2; (4) cpal streams already per-device rate → confirmed in `stream_builder.rs:139`, no code needed.
- Type consistency: `device_rates: &HashMap<DeviceId, f32>` is the single name/shape used in `build_per_input_runtimes`, `build_per_input_runtime_states`, `upsert_chain*`, `BuildRequest.device_sample_rates` (owned), and `ResolvedChainAudioConfig.by_device` (owned). Engine sample rate is `f32` everywhere; device rate cast `u32 as f32` at the resolve boundary only.
- Fallback invariant: every changed function defaults to the scalar `sample_rate` when the device is absent from the map, so an empty map reproduces today's behaviour exactly (single-binding bit-exactness).
