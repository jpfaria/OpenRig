# LV2 Modulation Block Models Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 8 LV2 modulation plugins as block models in `block-mod`, completing the first batch of issue #54.

**Architecture:** Each plugin is a standalone `lv2_*.rs` file with a `MODEL_DEFINITION` constant that gets auto-registered by `build.rs`. Mono plugins use DualMono pattern (two LV2 instances for stereo). True stereo plugins use `build_stereo_lv2_processor`. The `lv2` crate dependency and `Lv2` backend variant must be added to `block-mod` first.

**Tech Stack:** Rust, LV2 (via `crates/lv2`), Slint (UI auto-picks up new models)

**Workspace:** `.solvers/issue-54/` — NEVER edit the main workspace.

---

### Task 1: Add LV2 support to block-mod crate

**Files:**
- Modify: `crates/block-mod/Cargo.toml`
- Modify: `crates/block-mod/src/lib.rs`

- [ ] **Step 1: Add lv2 dependency to Cargo.toml**

```toml
[dependencies]
anyhow.workspace = true
block-core = { path = "../block-core" }
lv2 = { path = "../lv2" }
```

- [ ] **Step 2: Add Lv2 variant to ModBackendKind and update match**

In `crates/block-mod/src/lib.rs`, add `Lv2` to the enum and the match arm:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ModBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
}
```

And in `mod_model_visual`:

```rust
ModBackendKind::Native => "NATIVE",
ModBackendKind::Nam => "NAM",
ModBackendKind::Ir => "IR",
ModBackendKind::Lv2 => "LV2",
```

- [ ] **Step 3: Build to verify**

```bash
cd .solvers/issue-54 && cargo build -p block-mod
```

Expected: compiles with zero errors, zero warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/block-mod/Cargo.toml crates/block-mod/src/lib.rs
git commit -m "feat(block-mod): add Lv2 backend kind for LV2 plugin support"
```

---

### Task 2: TAP Chorus/Flanger (true stereo)

**Files:**
- Create: `crates/block-mod/src/lv2_tap_chorus_flanger.rs`

- [ ] **Step 1: Create the model file**

```rust
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_tap_chorus_flanger";
pub const DISPLAY_NAME: &str = "TAP Chorus/Flanger";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/chorusflanger";
const PLUGIN_DIR: &str = "tap-chorusflanger.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_chorusflanger.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_chorusflanger.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_chorusflanger.dll";

// LV2 port indices (from TTL)
const PORT_FREQUENCY: usize = 0;
const PORT_LR_PHASE: usize = 1;
const PORT_DEPTH: usize = 2;
const PORT_DELAY: usize = 3;
const PORT_CONTOUR: usize = 4;
const PORT_DRY_LEVEL: usize = 5;
const PORT_WET_LEVEL: usize = 6;
const PORT_LEFT_IN: usize = 7;
const PORT_RIGHT_IN: usize = 8;
const PORT_LEFT_OUT: usize = 9;
const PORT_RIGHT_OUT: usize = 10;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "modulation".into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter("frequency", "Frequency", None, Some(1.75), 0.0, 5.0, 0.01, ParameterUnit::Hertz),
            float_parameter("lr_phase", "L/R Phase", None, Some(90.0), 0.0, 180.0, 1.0, ParameterUnit::None),
            float_parameter("depth", "Depth", None, Some(75.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("delay", "Delay", None, Some(25.0), 0.0, 100.0, 0.1, ParameterUnit::Milliseconds),
            float_parameter("dry_level", "Dry Level", None, Some(-3.0), -90.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("wet_level", "Wet Level", None, Some(-3.0), -90.0, 20.0, 0.1, ParameterUnit::Decibels),
        ],
    }
}

fn resolve_lib_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let candidates = [
        exe_dir.as_ref().map(|d| d.join("../../").join(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
        Some(std::path::PathBuf::from(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    anyhow::bail!("LV2 binary '{}' not found in '{}'", PLUGIN_BINARY, lv2::default_lv2_lib_dir())
}

fn resolve_bundle_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let candidates = [
        exe_dir.as_ref().map(|d| d.join("../../plugins").join(PLUGIN_DIR)),
        Some(std::path::PathBuf::from("plugins").join(PLUGIN_DIR)),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    anyhow::bail!("LV2 bundle '{}' not found in plugins/", PLUGIN_DIR)
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let frequency = required_f32(params, "frequency").map_err(anyhow::Error::msg)?;
    let lr_phase = required_f32(params, "lr_phase").map_err(anyhow::Error::msg)?;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)?;
    let delay = required_f32(params, "delay").map_err(anyhow::Error::msg)?;
    let dry_level = required_f32(params, "dry_level").map_err(anyhow::Error::msg)?;
    let wet_level = required_f32(params, "wet_level").map_err(anyhow::Error::msg)?;

    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;
    let controls = &[
        (PORT_FREQUENCY, frequency),
        (PORT_LR_PHASE, lr_phase),
        (PORT_DEPTH, depth),
        (PORT_DELAY, delay),
        (PORT_CONTOUR, 100.0), // fixed contour
        (PORT_DRY_LEVEL, dry_level),
        (PORT_WET_LEVEL, wet_level),
    ];

    match layout {
        AudioChannelLayout::Mono => {
            let processor = lv2::build_lv2_processor(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_LEFT_IN], &[PORT_LEFT_OUT], controls,
            )?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let processor = lv2::build_stereo_lv2_processor(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_LEFT_IN, PORT_RIGHT_IN], &[PORT_LEFT_OUT, PORT_RIGHT_OUT], controls,
            )?;
            Ok(BlockProcessor::Stereo(Box::new(processor)))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: ModModelDefinition = ModModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: ModBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
```

- [ ] **Step 2: Build to verify**

```bash
cargo build -p block-mod
```

Expected: compiles with zero errors, zero warnings.

- [ ] **Step 3: Commit**

```bash
git add crates/block-mod/src/lv2_tap_chorus_flanger.rs
git commit -m "feat(block-mod): add TAP Chorus/Flanger LV2 model"
```

---

### Task 3: TAP Tremolo (mono/DualMono)

**Files:**
- Create: `crates/block-mod/src/lv2_tap_tremolo.rs`

- [ ] **Step 1: Create the model file**

```rust
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_tap_tremolo";
pub const DISPLAY_NAME: &str = "TAP Tremolo";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/tremolo";
const PLUGIN_DIR: &str = "tap-tremolo.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_tremolo.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_tremolo.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_tremolo.dll";

const PORT_FREQUENCY: usize = 0;
const PORT_DEPTH: usize = 1;
const PORT_GAIN: usize = 2;
const PORT_AUDIO_IN: usize = 3;
const PORT_AUDIO_OUT: usize = 4;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "modulation".into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("frequency", "Frequency", None, Some(5.0), 0.0, 20.0, 0.1, ParameterUnit::Hertz),
            float_parameter("depth", "Depth", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("gain", "Gain", None, Some(0.0), -70.0, 20.0, 0.1, ParameterUnit::Decibels),
        ],
    }
}

fn resolve_lib_path() -> Result<String> {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let candidates = [
        exe_dir.as_ref().map(|d| d.join("../../").join(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
        Some(std::path::PathBuf::from(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.exists() { return Ok(candidate.to_string_lossy().to_string()); }
    }
    anyhow::bail!("LV2 binary '{}' not found in '{}'", PLUGIN_BINARY, lv2::default_lv2_lib_dir())
}

fn resolve_bundle_path() -> Result<String> {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let candidates = [
        exe_dir.as_ref().map(|d| d.join("../../plugins").join(PLUGIN_DIR)),
        Some(std::path::PathBuf::from("plugins").join(PLUGIN_DIR)),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.exists() { return Ok(candidate.to_string_lossy().to_string()); }
    }
    anyhow::bail!("LV2 bundle '{}' not found in plugins/", PLUGIN_DIR)
}

struct DualMonoLv2 {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono_processor(sample_rate: f32, frequency: f32, depth: f32, gain: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;
    lv2::build_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_FREQUENCY, frequency), (PORT_DEPTH, depth), (PORT_GAIN, gain)],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let frequency = required_f32(params, "frequency").map_err(anyhow::Error::msg)?;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)?;
    let gain = required_f32(params, "gain").map_err(anyhow::Error::msg)?;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, frequency, depth, gain)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, frequency, depth, gain)?;
            let right = build_mono_processor(sample_rate, frequency, depth, gain)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: ModModelDefinition = ModModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: ModBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
```

- [ ] **Step 2: Build and commit**

```bash
cargo build -p block-mod
git add crates/block-mod/src/lv2_tap_tremolo.rs
git commit -m "feat(block-mod): add TAP Tremolo LV2 model"
```

---

### Task 4: CAPS PhaserII (mono/DualMono)

**Files:**
- Create: `crates/block-mod/src/lv2_caps_phaser2.rs`

- [ ] **Step 1: Create the model file**

Same DualMono pattern as Task 3. Key constants:

```rust
pub const MODEL_ID: &str = "lv2_caps_phaser2";
pub const DISPLAY_NAME: &str = "CAPS Phaser II";
const BRAND: &str = "caps";
const PLUGIN_URI: &str = "http://moddevices.com/plugins/caps/PhaserII";
const PLUGIN_DIR: &str = "mod-caps-PhaserII.lv2";
#[cfg(target_os = "macos")]   const PLUGIN_BINARY: &str = "PhaserII.dylib";
#[cfg(target_os = "linux")]   const PLUGIN_BINARY: &str = "PhaserII.so";
#[cfg(target_os = "windows")] const PLUGIN_BINARY: &str = "PhaserII.dll";

const PORT_RATE: usize = 0;
const PORT_LFO: usize = 1;
const PORT_DEPTH: usize = 2;
const PORT_SPREAD: usize = 3;
const PORT_RESONANCE: usize = 4;
const PORT_AUDIO_IN: usize = 5;
const PORT_AUDIO_OUT: usize = 6;
```

Parameters (all 0.0–1.0 normalized, use ParameterUnit::None):
- `rate` "Rate" default 0.25
- `depth` "Depth" default 0.75
- `spread` "Spread" default 0.75
- `resonance` "Resonance" default 0.25

LFO fixed to 0 (Sine). `audio_mode: ModelAudioMode::DualMono`.

- [ ] **Step 2: Build and commit**

```bash
cargo build -p block-mod
git add crates/block-mod/src/lv2_caps_phaser2.rs
git commit -m "feat(block-mod): add CAPS PhaserII LV2 model"
```

---

### Task 5: MDA ThruZero Flanger (true stereo)

**Files:**
- Create: `crates/block-mod/src/lv2_mda_thruzero.rs`

- [ ] **Step 1: Create the model file**

Same TrueStereo pattern as Task 2. Key constants:

```rust
pub const MODEL_ID: &str = "lv2_mda_thruzero";
pub const DISPLAY_NAME: &str = "MDA ThruZero";
const BRAND: &str = "mda";
const PLUGIN_URI: &str = "http://drobilla.net/plugins/mda/ThruZero";
const PLUGIN_DIR: &str = "mod-mda-ThruZero.lv2";
#[cfg(target_os = "macos")]   const PLUGIN_BINARY: &str = "ThruZero.dylib";
#[cfg(target_os = "linux")]   const PLUGIN_BINARY: &str = "ThruZero.so";
#[cfg(target_os = "windows")] const PLUGIN_BINARY: &str = "ThruZero.dll";

const PORT_RATE: usize = 0;
const PORT_DEPTH: usize = 1;
const PORT_MIX: usize = 2;
const PORT_FEEDBACK: usize = 3;
const PORT_DEPTH_MOD: usize = 4;
const PORT_LEFT_IN: usize = 5;
const PORT_RIGHT_IN: usize = 6;
const PORT_LEFT_OUT: usize = 7;
const PORT_RIGHT_OUT: usize = 8;
```

Parameters (all 0.0–1.0 normalized):
- `rate` "Rate" default 0.3
- `depth` "Depth" default 0.43
- `mix` "Mix" default 0.47
- `feedback` "Feedback" default 0.3

`depth_mod` fixed to 1.0. `audio_mode: ModelAudioMode::TrueStereo`.

- [ ] **Step 2: Build and commit**

```bash
cargo build -p block-mod
git add crates/block-mod/src/lv2_mda_thruzero.rs
git commit -m "feat(block-mod): add MDA ThruZero LV2 model"
```

---

### Task 6: SHIRO Harmless (true stereo)

**Files:**
- Create: `crates/block-mod/src/lv2_harmless.rs`

- [ ] **Step 1: Create the model file**

TrueStereo pattern. Key constants:

```rust
pub const MODEL_ID: &str = "lv2_harmless";
pub const DISPLAY_NAME: &str = "Harmless";
const BRAND: &str = "shiro";
const PLUGIN_URI: &str = "https://github.com/ninodewit/SHIRO-Plugins/plugins/harmless";
const PLUGIN_DIR: &str = "Harmless.lv2";
#[cfg(target_os = "macos")]   const PLUGIN_BINARY: &str = "Harmless_dsp.dylib";
#[cfg(target_os = "linux")]   const PLUGIN_BINARY: &str = "Harmless_dsp.so";
#[cfg(target_os = "windows")] const PLUGIN_BINARY: &str = "Harmless_dsp.dll";

const PORT_LEFT_IN: usize = 0;
const PORT_RIGHT_IN: usize = 1;
const PORT_LEFT_OUT: usize = 2;
const PORT_RIGHT_OUT: usize = 3;
const PORT_RATE: usize = 4;
const PORT_SHAPE: usize = 5;
const PORT_TONE: usize = 6;
const PORT_PHASE: usize = 7;
const PORT_DEPTH: usize = 8;
```

Parameters:
- `rate` "Rate" default 4.0, min 0.1, max 20.0 Hz
- `shape` "Shape" default 50.0, min 1.0, max 99.0 % (maps 0.01–0.99)
- `tone` "Tone" default 6000.0, min 500.0, max 6000.0 Hz
- `phase` "Phase" default 0.0, min -180.0, max 180.0
- `depth` "Depth" default 100.0, min 0.0, max 100.0 %

`audio_mode: ModelAudioMode::TrueStereo`.

- [ ] **Step 2: Build and commit**

```bash
cargo build -p block-mod
git add crates/block-mod/src/lv2_harmless.rs
git commit -m "feat(block-mod): add SHIRO Harmless LV2 model"
```

---

### Task 7: SHIRO Larynx (mono/DualMono)

**Files:**
- Create: `crates/block-mod/src/lv2_larynx.rs`

- [ ] **Step 1: Create the model file**

DualMono pattern. Key constants:

```rust
pub const MODEL_ID: &str = "lv2_larynx";
pub const DISPLAY_NAME: &str = "Larynx";
const BRAND: &str = "shiro";
const PLUGIN_URI: &str = "https://github.com/ninodewit/SHIRO-Plugins/plugins/larynx";
const PLUGIN_DIR: &str = "Larynx.lv2";
#[cfg(target_os = "macos")]   const PLUGIN_BINARY: &str = "Larynx_dsp.dylib";
#[cfg(target_os = "linux")]   const PLUGIN_BINARY: &str = "Larynx_dsp.so";
#[cfg(target_os = "windows")] const PLUGIN_BINARY: &str = "Larynx_dsp.dll";

const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_TONE: usize = 2;
const PORT_DEPTH: usize = 3;
const PORT_RATE: usize = 4;
```

Parameters:
- `tone` "Tone" default 6000.0, min 500.0, max 12000.0 Hz
- `depth` "Depth" default 1.0, min 0.1, max 5.0 ms
- `rate` "Rate" default 5.0, min 0.1, max 10.0 Hz

`audio_mode: ModelAudioMode::DualMono`.

- [ ] **Step 2: Build and commit**

```bash
cargo build -p block-mod
git add crates/block-mod/src/lv2_larynx.rs
git commit -m "feat(block-mod): add SHIRO Larynx LV2 model"
```

---

### Task 8: fomp CS Chorus (mono/DualMono)

**Files:**
- Create: `crates/block-mod/src/lv2_fomp_cs_chorus.rs`

- [ ] **Step 1: Create the model file**

DualMono pattern. Key constants:

```rust
pub const MODEL_ID: &str = "lv2_fomp_cs_chorus";
pub const DISPLAY_NAME: &str = "CS Chorus";
const BRAND: &str = "fomp";
const PLUGIN_URI: &str = "http://drobilla.net/plugins/fomp/cs_chorus1";
const PLUGIN_DIR: &str = "fomp.lv2";
#[cfg(target_os = "macos")]   const PLUGIN_BINARY: &str = "cs_chorus.dylib";
#[cfg(target_os = "linux")]   const PLUGIN_BINARY: &str = "cs_chorus.so";
#[cfg(target_os = "windows")] const PLUGIN_BINARY: &str = "cs_chorus.dll";

const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_DELAY: usize = 2;
const PORT_MOD_FREQ_1: usize = 3;
const PORT_MOD_AMP_1: usize = 4;
const PORT_MOD_FREQ_2: usize = 5;
const PORT_MOD_AMP_2: usize = 6;
```

Parameters:
- `delay` "Delay" default 1.0, min 0.0, max 30.0 ms
- `mod_freq_1` "Mod Frequency 1" default 0.25, min 0.003, max 10.0 Hz
- `mod_amp_1` "Mod Amplitude 1" default 1.0, min 0.0, max 10.0 ms
- `mod_freq_2` "Mod Frequency 2" default 0.125, min 0.01, max 30.0 Hz
- `mod_amp_2` "Mod Amplitude 2" default 0.5, min 0.0, max 3.0 ms

`audio_mode: ModelAudioMode::DualMono`.

- [ ] **Step 2: Build and commit**

```bash
cargo build -p block-mod
git add crates/block-mod/src/lv2_fomp_cs_chorus.rs
git commit -m "feat(block-mod): add fomp CS Chorus LV2 model"
```

---

### Task 9: fomp CS Phaser (mono/DualMono)

**Files:**
- Create: `crates/block-mod/src/lv2_fomp_cs_phaser.rs`

- [ ] **Step 1: Create the model file**

DualMono pattern. Key constants:

```rust
pub const MODEL_ID: &str = "lv2_fomp_cs_phaser";
pub const DISPLAY_NAME: &str = "CS Phaser";
const BRAND: &str = "fomp";
const PLUGIN_URI: &str = "http://drobilla.net/plugins/fomp/cs_phaser1";
const PLUGIN_DIR: &str = "fomp.lv2";
#[cfg(target_os = "macos")]   const PLUGIN_BINARY: &str = "cs_phaser.dylib";
#[cfg(target_os = "linux")]   const PLUGIN_BINARY: &str = "cs_phaser.so";
#[cfg(target_os = "windows")] const PLUGIN_BINARY: &str = "cs_phaser.dll";

const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_FM: usize = 2;
const PORT_EXP_FM: usize = 3;
const PORT_LIN_FM: usize = 4;
const PORT_INPUT_GAIN: usize = 5;
const PORT_SECTIONS: usize = 6;
const PORT_FREQUENCY: usize = 7;
const PORT_EXP_FM_GAIN: usize = 8;
const PORT_LIN_FM_GAIN: usize = 9;
const PORT_FEEDBACK: usize = 10;
const PORT_OUTPUT_MIX: usize = 11;
```

Parameters (exposed to user):
- `input_gain` "Input Gain" default 0.0, min -40.0, max 10.0 dB
- `sections` "Sections" default 2.0, min 1.0, max 30.0 (integer steps)
- `frequency` "Frequency" default 0.0, min -6.0, max 6.0
- `feedback` "Feedback" default 0.0, min -1.0, max 1.0
- `output_mix` "Output Mix" default 0.0, min -1.0, max 1.0

CV ports (FM, Exp FM, Lin FM) and their gains fixed to 0.0. `audio_mode: ModelAudioMode::DualMono`.

Controls array in build must include all ports:
```rust
&[
    (PORT_FM, 0.0),
    (PORT_EXP_FM, 0.0),
    (PORT_LIN_FM, 0.0),
    (PORT_INPUT_GAIN, input_gain),
    (PORT_SECTIONS, sections),
    (PORT_FREQUENCY, frequency),
    (PORT_EXP_FM_GAIN, 0.0),
    (PORT_LIN_FM_GAIN, 0.0),
    (PORT_FEEDBACK, feedback),
    (PORT_OUTPUT_MIX, output_mix),
]
```

- [ ] **Step 2: Build and commit**

```bash
cargo build -p block-mod
git add crates/block-mod/src/lv2_fomp_cs_phaser.rs
git commit -m "feat(block-mod): add fomp CS Phaser LV2 model"
```

---

### Task 10: Final build, push and PR

- [ ] **Step 1: Full build and verify all 8 models appear**

```bash
cargo build -p block-mod 2>&1 | tail -5
```

Expected: zero errors, zero warnings.

- [ ] **Step 2: Push and create PR**

```bash
git push -u origin feature/issue-54-lv2-modulation
```

Then provide checkout command:
```bash
git checkout feature/issue-54-lv2-modulation && git pull origin feature/issue-54-lv2-modulation
```
