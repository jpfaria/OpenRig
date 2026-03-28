# Chain I/O Redesign — Multiple Inputs/Outputs per Chain

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace single-input/single-output chain model with N inputs and M outputs, each independently configured with device, mode (mono/stereo), and channels.

**Architecture:** Each chain owns a list of `ChainInput` and `ChainOutput` structs. Each input spawns its own processing instance (clone of the block chain). All processing outputs are mixed and routed to each output. YAML backward-compatible via serde migration.

**Tech Stack:** Rust, Slint UI, CPAL audio, serde YAML

**Branch:** `feature/issue-2`

---

### Task 1: New data model structs

**Files:**
- Modify: `crates/project/src/chain.rs`

- [ ] **Step 1: Define ChainInput and ChainOutput structs**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainInput {
    #[serde(default = "default_input_name")]
    pub name: String,
    pub device_id: DeviceId,
    pub mode: ChainInputMode,
    pub channels: Vec<usize>,
}

fn default_input_name() -> String {
    "Input".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainOutput {
    #[serde(default = "default_output_name")]
    pub name: String,
    pub device_id: DeviceId,
    pub mode: ChainOutputMode,
    pub channels: Vec<usize>,
}

fn default_output_name() -> String {
    "Output".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChainOutputMode {
    Mono,
    #[default]
    Stereo,
}
```

- [ ] **Step 2: Update Chain struct with new fields, keep old fields for backward compat**

```rust
pub struct Chain {
    pub id: ChainId,
    pub description: Option<String>,
    pub instrument: String,
    pub enabled: bool,
    // New fields
    #[serde(default)]
    pub inputs: Vec<ChainInput>,
    #[serde(default)]
    pub outputs: Vec<ChainOutput>,
    pub blocks: Vec<AudioBlock>,
    // Legacy fields — used for migration, skip serialization
    #[serde(default, skip_serializing)]
    pub input_device_id: DeviceId,
    #[serde(default, skip_serializing)]
    pub input_channels: Vec<usize>,
    #[serde(default, skip_serializing)]
    pub output_device_id: DeviceId,
    #[serde(default, skip_serializing)]
    pub output_channels: Vec<usize>,
    #[serde(default, skip_serializing)]
    pub output_mixdown: ChainOutputMixdown,
    #[serde(default, skip_serializing)]
    pub input_mode: ChainInputMode,
}
```

- [ ] **Step 3: Add migration function**

```rust
impl Chain {
    /// Migrate legacy single-input/output to new multi-input/output model.
    /// Called after deserialization.
    pub fn migrate_legacy_io(&mut self) {
        if self.inputs.is_empty() && !self.input_device_id.0.is_empty() {
            self.inputs.push(ChainInput {
                name: "Input 1".to_string(),
                device_id: self.input_device_id.clone(),
                mode: self.input_mode,
                channels: self.input_channels.clone(),
            });
        }
        if self.outputs.is_empty() && !self.output_device_id.0.is_empty() {
            let mode = if self.output_channels.len() >= 2 {
                ChainOutputMode::Stereo
            } else {
                ChainOutputMode::Mono
            };
            self.outputs.push(ChainOutput {
                name: "Output 1".to_string(),
                device_id: self.output_device_id.clone(),
                mode,
                channels: self.output_channels.clone(),
            });
        }
    }

    /// Validate that no two inputs share the same device+channel.
    pub fn validate_channel_conflicts(&self) -> Result<(), String> {
        let mut used: Vec<(String, usize)> = Vec::new();
        for input in &self.inputs {
            for &ch in &input.channels {
                let key = (input.device_id.0.clone(), ch);
                if used.contains(&key) {
                    return Err(format!(
                        "Channel {} on device '{}' is used by multiple inputs",
                        ch, input.device_id.0
                    ));
                }
                used.push(key);
            }
        }
        // Same for outputs
        let mut used: Vec<(String, usize)> = Vec::new();
        for output in &self.outputs {
            for &ch in &output.channels {
                let key = (output.device_id.0.clone(), ch);
                if used.contains(&key) {
                    return Err(format!(
                        "Channel {} on device '{}' is used by multiple outputs",
                        ch, output.device_id.0
                    ));
                }
                used.push(key);
            }
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Update processing_layout to work with ChainInput**

```rust
pub fn processing_layout_for_input(input: &ChainInput) -> ProcessingLayout {
    let ch_count = input.channels.len();
    match input.mode {
        ChainInputMode::DualMono if ch_count >= 2 => ProcessingLayout::DualMono,
        ChainInputMode::Stereo if ch_count >= 2 => ProcessingLayout::Stereo,
        _ => ProcessingLayout::Mono,
    }
}
```

- [ ] **Step 5: Run `cargo build` — verify zero warnings**

Run: `cargo build 2>&1 | tail -5`

- [ ] **Step 6: Commit**

```bash
git add crates/project/src/chain.rs
git commit -m "feat(project): add ChainInput/ChainOutput structs with legacy migration (#2)"
```

---

### Task 2: YAML migration — load old format, save new format

**Files:**
- Modify: `crates/infra-yaml/src/lib.rs`

- [ ] **Step 1: Call migrate_legacy_io after deserializing chains**

Find where chains are loaded (the function that deserializes `project.yaml`). After loading, iterate chains and call `chain.migrate_legacy_io()`.

```rust
// After deserializing project:
for chain in &mut project.chains {
    chain.migrate_legacy_io();
}
```

- [ ] **Step 2: Update existing project.yaml and preset.yaml in the repo**

Convert the YAML files to use the new `inputs`/`outputs` format so tests and default projects work correctly.

- [ ] **Step 3: Run `cargo build` — verify zero warnings**

- [ ] **Step 4: Commit**

```bash
git add crates/infra-yaml/src/lib.rs project.yaml preset.yaml
git commit -m "feat(yaml): migrate legacy chain I/O to multi-input/output format (#2)"
```

---

### Task 3: Engine — multiple processing instances per chain

**Files:**
- Modify: `crates/engine/src/runtime.rs`

This is the most complex task. Each ChainInput gets its own block chain instance.

- [ ] **Step 1: Create InputProcessingState struct**

```rust
struct InputProcessingState {
    input_read_layout: AudioChannelLayout,
    processing_layout: AudioChannelLayout,
    input_channels: Vec<usize>,
    blocks: Vec<BlockRuntimeNode>,
    frame_buffer: Vec<AudioFrame>,
    fade_in_remaining: usize,
}
```

- [ ] **Step 2: Create OutputRoutingState struct**

```rust
struct OutputRoutingState {
    output_layout: AudioChannelLayout,
    output_channels: Vec<usize>,
    output_mixdown: ChainOutputMixdown,
    queue: VecDeque<AudioFrame>,
}
```

- [ ] **Step 3: Refactor ChainProcessingState to hold Vec of inputs**

```rust
struct ChainProcessingState {
    input_states: Vec<InputProcessingState>,
    tuner_samples: Vec<f32>,
    // Mixed output from all inputs
    mixed_buffer: Vec<AudioFrame>,
}

struct ChainOutputState {
    output_routes: Vec<OutputRoutingState>,
}
```

- [ ] **Step 4: Update build_chain_runtime_state**

For each `chain.inputs`, build a separate `InputProcessingState` with its own block instances. For each `chain.outputs`, build an `OutputRoutingState`.

```rust
pub fn build_chain_runtime_state(chain: &Chain, sample_rate: f32) -> Result<ChainRuntimeState> {
    let mut input_states = Vec::new();
    for input in &chain.inputs {
        let layout = processing_layout_for_input(input);
        let processing_layout_channel = match layout {
            ProcessingLayout::Mono | ProcessingLayout::DualMono => AudioChannelLayout::Mono,
            ProcessingLayout::Stereo => AudioChannelLayout::Stereo,
        };
        let input_read_layout = match input.mode {
            ChainInputMode::Mono => AudioChannelLayout::Mono,
            ChainInputMode::Stereo | ChainInputMode::DualMono => AudioChannelLayout::Stereo,
        };
        // Build blocks for this input instance
        let blocks = build_blocks_for_chain(chain, processing_layout_channel, sample_rate)?;
        input_states.push(InputProcessingState {
            input_read_layout,
            processing_layout: processing_layout_channel,
            input_channels: input.channels.clone(),
            blocks,
            frame_buffer: Vec::with_capacity(1024),
            fade_in_remaining: FADE_IN_FRAMES,
        });
    }

    let mut output_routes = Vec::new();
    for output in &chain.outputs {
        let output_layout = if output.channels.len() >= 2 {
            match output.mode {
                ChainOutputMode::Stereo => AudioChannelLayout::Stereo,
                ChainOutputMode::Mono => AudioChannelLayout::Mono,
            }
        } else {
            AudioChannelLayout::Mono
        };
        output_routes.push(OutputRoutingState {
            output_layout,
            output_channels: output.channels.clone(),
            output_mixdown: ChainOutputMixdown::Average,
            queue: VecDeque::with_capacity(MAX_BUFFERED_OUTPUT_FRAMES),
        });
    }

    Ok(ChainRuntimeState {
        processing: Mutex::new(ChainProcessingState {
            input_states,
            tuner_samples: Vec::new(),
            mixed_buffer: Vec::with_capacity(1024),
        }),
        output: Mutex::new(ChainOutputState { output_routes }),
        tuner_shared_buffer: Mutex::new(Vec::new()),
        tuner_reading: Mutex::new(block_util::TunerReading::default()),
    })
}
```

- [ ] **Step 5: Extract block building into helper function**

```rust
fn build_blocks_for_chain(
    chain: &Chain,
    layout: AudioChannelLayout,
    sample_rate: f32,
) -> Result<Vec<BlockRuntimeNode>> {
    // Move existing block-building logic here
    // This is called once per input instance
}
```

- [ ] **Step 6: Update process_input_f32 to route to correct InputProcessingState**

The input callback from CPAL needs to know which input group it belongs to. Add an `input_index: usize` parameter:

```rust
pub fn process_input_f32(
    runtime: &Arc<ChainRuntimeState>,
    input_index: usize,
    data: &[f32],
    input_total_channels: usize,
) {
    let mut processing = match runtime.processing.try_lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    let input_state = match processing.input_states.get_mut(input_index) {
        Some(s) => s,
        None => return,
    };
    // Process this input's frames through its block chain
    // Then push to mixed_buffer
    // Then distribute to all output routes
}
```

- [ ] **Step 7: Update process_output_f32 to use output_index**

```rust
pub fn process_output_f32(
    runtime: &Arc<ChainRuntimeState>,
    output_index: usize,
    out: &mut [f32],
    output_total_channels: usize,
) {
    let mut output_state = match runtime.output.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            out.fill(0.0);
            return;
        }
    };
    let route = match output_state.output_routes.get_mut(output_index) {
        Some(r) => r,
        None => {
            out.fill(0.0);
            return;
        }
    };
    // Pop from this route's queue and write to output buffer
}
```

- [ ] **Step 8: Update update_chain_runtime_state for multi-input**

- [ ] **Step 9: Run `cargo build` — verify zero warnings**

- [ ] **Step 10: Run `cargo test --package engine` — verify existing tests pass (update as needed)**

- [ ] **Step 11: Commit**

```bash
git add crates/engine/src/runtime.rs
git commit -m "feat(engine): support multiple input/output instances per chain (#2)"
```

---

### Task 4: CPAL — multiple streams per chain

**Files:**
- Modify: `crates/infra-cpal/src/lib.rs`

- [ ] **Step 1: Update build_chain_streams to create one stream per input/output**

Currently returns `(Stream, Stream)`. Change to return `(Vec<Stream>, Vec<Stream>)`:

```rust
pub fn build_chain_streams(
    chain_id: &ChainId,
    chain: &Chain,
    runtime: Arc<ChainRuntimeState>,
    // ... device resolution params
) -> Result<(Vec<Stream>, Vec<Stream>)> {
    let mut input_streams = Vec::new();
    for (i, input) in chain.inputs.iter().enumerate() {
        let resolved = resolve_input_device(&input.device_id, ...)?;
        let stream = build_input_stream_for_input(
            chain_id, i, &input, resolved, runtime.clone()
        )?;
        input_streams.push(stream);
    }

    let mut output_streams = Vec::new();
    for (i, output) in chain.outputs.iter().enumerate() {
        let resolved = resolve_output_device(&output.device_id, ...)?;
        let stream = build_output_stream_for_output(
            chain_id, i, &output, resolved, runtime.clone()
        )?;
        output_streams.push(stream);
    }

    Ok((input_streams, output_streams))
}
```

- [ ] **Step 2: Update input stream callback to pass input_index**

```rust
fn build_input_stream_for_input(
    chain_id: &ChainId,
    input_index: usize,
    input: &ChainInput,
    resolved: ResolvedInputDevice,
    runtime: Arc<ChainRuntimeState>,
) -> Result<Stream> {
    // ... same as current build_input_stream_for_chain but
    // callback calls process_input_f32(runtime, input_index, data, channels)
}
```

- [ ] **Step 3: Update output stream callback to pass output_index**

Similar to input — pass `output_index` to `process_output_f32`.

- [ ] **Step 4: Update ChainStreamSignature for multiple streams**

- [ ] **Step 5: Update start_audio / stop_audio to handle Vec of streams**

- [ ] **Step 6: Run `cargo build` — verify zero warnings**

- [ ] **Step 7: Commit**

```bash
git add crates/infra-cpal/src/lib.rs
git commit -m "feat(cpal): create per-input and per-output audio streams (#2)"
```

---

### Task 5: GUI — Chain Editor with add/remove inputs and outputs

**Files:**
- Modify: `crates/adapter-gui/src/lib.rs`
- Modify: `crates/adapter-gui/ui/pages/chain_editor.slint`
- Modify: `crates/adapter-gui/ui/pages/chain_endpoint_editor.slint`
- Modify: `crates/adapter-gui/ui/app-window.slint`

- [ ] **Step 1: Update ChainDraft to use input/output groups**

```rust
struct InputGroupDraft {
    name: String,
    device_id: Option<String>,
    channels: Vec<usize>,
    mode: ChainInputMode,
}

struct OutputGroupDraft {
    name: String,
    device_id: Option<String>,
    channels: Vec<usize>,
    mode: ChainOutputMode,
}

struct ChainDraft {
    editing_index: Option<usize>,
    name: String,
    instrument: String,
    inputs: Vec<InputGroupDraft>,
    outputs: Vec<OutputGroupDraft>,
}
```

- [ ] **Step 2: Update chain_editor.slint to show list of inputs/outputs with add/remove**

Replace single "Entrada" / "Saída" rows with a list of input groups and output groups, each with an "edit" button and a "+" button to add new groups.

- [ ] **Step 3: Update chain_endpoint_editor.slint to edit a single input/output group**

The existing endpoint editor already has device selection and channel toggles. Adapt it to receive which group index is being edited.

- [ ] **Step 4: Wire callbacks for add/remove/edit input and output groups**

- [ ] **Step 5: Update save-chain logic to construct Chain with inputs/outputs from draft**

- [ ] **Step 6: Run `cargo build` — verify zero warnings**

- [ ] **Step 7: Visual test — verify UI works**

- [ ] **Step 8: Commit**

```bash
git add crates/adapter-gui/
git commit -m "feat(gui): multi-input/output chain editor UI (#2)"
```

---

### Task 6: Update CLAUDE.md and tests

**Files:**
- Modify: `CLAUDE.md`
- Modify: `crates/engine/src/runtime.rs` (tests section)

- [ ] **Step 1: Update CLAUDE.md chain documentation**

Update the "Configuração de áudio" section and chain model description.

- [ ] **Step 2: Update/add engine tests for multi-input/output**

- [ ] **Step 3: Run full test suite**

Run: `cargo test 2>&1 | tail -20`

- [ ] **Step 4: Final `cargo build` — zero warnings**

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md crates/engine/src/runtime.rs
git commit -m "docs: update CLAUDE.md and tests for multi-input/output chains (#2)"
```

---

### Task 7: Create PR

- [ ] **Step 1: Push branch and create PR**

```bash
git push origin feature/issue-2
gh pr create --base develop --head feature/issue-2 \
  --title "feat: redesign chain I/O for multiple inputs/outputs" \
  --body "Closes #2"
```
