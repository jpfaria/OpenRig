# VST3 Plugin Integration

**Issue:** #178
**Date:** 2026-04-07
**Status:** Draft

---

## Summary

Integrar suporte a plugins VST3 no OpenRig com duas abordagens complementares:

1. **Bundled open-source** — plugins curados pré-compilados e distribuídos com o app (análogo ao LV2 atual)
2. **Dynamic loading** — carregar qualquer plugin VST3 instalado pelo usuário no sistema

---

## VST3 vs LV2 — Diferenças arquiteturais

| Aspecto | LV2 (atual) | VST3 (proposto) |
|---------|-------------|-----------------|
| API | C puro | C++ COM-like (interfaces via vtables) |
| Discovery | Nenhum (estático) | Scan de paths padrão por plataforma |
| Params | Hardcoded no model definition | Enumerados em runtime via `IEditController` |
| Bundle | Diretório `.lv2/` + TTL | Diretório `.vst3/` com estrutura padrão |
| Entry point | `lv2_descriptor()` export | `GetPluginFactory()` export |
| Threading | Single instance, single thread | `IComponent` + `IAudioProcessor` separados |
| State | Não suportado | `IEditController::getParamNormalized()` |

### VST3 Bundle Structure (macOS exemplo)

```
CloudSeed.vst3/
  Contents/
    MacOS/
      CloudSeed          ← dylib (sem extensão no macOS)
    Info.plist
    PkgInfo
```

### VST3 Key Interfaces

```
GetPluginFactory()              ← entry point exportado
  └─ IPluginFactory
       ├─ getFactoryInfo()      ← vendor, email, url
       ├─ countClasses()        ← quantos plugins no bundle
       └─ getClassInfo(i)       ← nome, category, classID (16 bytes = plugin UID)
            └─ createInstance(classID, IAudioProcessor)
                 └─ IComponent
                      ├─ initialize(FUnknown* context)
                      ├─ getBusCount(Audio, Input/Output)
                      ├─ getBusInfo(...)
                      └─ queryInterface(IEditController)
                           ├─ getParameterCount()
                           ├─ getParameterInfo(i) → name, id, flags, min, max, default
                           ├─ getParamNormalized(id) → 0.0..1.0
                           └─ setParamNormalized(id, value)
                 └─ IAudioProcessor
                      ├─ setupProcessing(ProcessSetup)
                      ├─ setProcessing(true)
                      └─ process(ProcessData)  ← audio callback
```

---

## Design Decisions

| Decisão | Escolha | Motivo |
|---------|---------|--------|
| FFI approach | `vst3-sys` crate (bindings C++) | Mais completo que FFI manual; ativo no crates.io |
| Abordagem bundled | Params hardcoded (como LV2) | Consistência com arquitetura atual; UX previsível |
| Abordagem dynamic | Params enumerados em runtime | Flexibilidade; suporta qualquer plugin do usuário |
| Backend kind | Variante `Vst3` nos enums existentes | Não criar novo bloco top-level para bundled |
| Dynamic block type | Novo `Vst3Dynamic` em `AudioBlockKind` | Necessário pois params são dinâmicos |
| Thread model | Processar apenas no audio thread | VST3 exige separação UI/audio |
| Falha de carregamento | Bypass silencioso + log de erro | Não crashar se plugin ausente |

---

## Fase 1 — `crates/vst3` (host core)

### Estrutura do crate

```
crates/vst3/
  src/
    lib.rs           ← public API
    host.rs          ← VST3 plugin loading via vst3-sys FFI
    processor.rs     ← Mono processor wrapper
    stereo.rs        ← Stereo processor wrapper
    discovery.rs     ← scan de paths do sistema
    params.rs        ← enumeração e mapeamento de parâmetros
  Cargo.toml
```

### `Cargo.toml`

```toml
[dependencies]
vst3-sys = "0.7"
anyhow = "1"
log = "0.4"
block-core = { path = "../block-core" }
```

### `host.rs` — Plugin loading

```rust
pub struct Vst3Plugin {
    factory: *mut IPluginFactory,
    component: *mut IComponent,
    processor: *mut IAudioProcessor,
    controller: *mut IEditController,
    _lib: libloading::Library,
}

impl Vst3Plugin {
    /// Carrega um plugin VST3.
    /// - `bundle_path`: caminho para o diretório `.vst3`
    /// - `plugin_uid`: 16 bytes do classID (do IPluginFactory::getClassInfo)
    pub fn load(bundle_path: &Path, plugin_uid: &[u8; 16], sample_rate: f64) -> Result<Self>
    
    pub fn param_count(&self) -> i32
    pub fn param_info(&self, index: i32) -> Result<Vst3ParamInfo>
    pub fn set_param(&self, param_id: u32, normalized: f64)
    pub fn get_param(&self, param_id: u32) -> f64
    pub fn process_audio(&mut self, input: &[f32], output: &mut [f32], n_samples: usize)
}

pub struct Vst3ParamInfo {
    pub id: u32,
    pub title: String,
    pub short_title: String,
    pub units: String,
    pub step_count: i32,     // 0 = continuous
    pub default_normalized: f64,
    pub flags: i32,
}
```

### `discovery.rs` — Scan de paths

```rust
/// Paths padrão VST3 por plataforma
pub fn system_vst3_paths() -> Vec<PathBuf> {
    // macOS
    #[cfg(target_os = "macos")]
    return vec![
        dirs::home_dir().unwrap().join("Library/Audio/Plug-Ins/VST3"),
        PathBuf::from("/Library/Audio/Plug-Ins/VST3"),
        PathBuf::from("/Network/Library/Audio/Plug-Ins/VST3"),
    ];
    
    // Windows
    #[cfg(target_os = "windows")]
    return vec![
        dirs::data_dir().unwrap().join("Common Files/VST3"),
        dirs::home_dir().unwrap().join("Documents/VST3"),
    ];
    
    // Linux
    #[cfg(target_os = "linux")]
    return vec![
        dirs::home_dir().unwrap().join(".vst3"),
        PathBuf::from("/usr/lib/vst3"),
        PathBuf::from("/usr/local/lib/vst3"),
    ];
}

/// Escaneia um diretório e retorna todos os plugins VST3 encontrados.
pub fn scan_vst3_dir(dir: &Path) -> Vec<Vst3PluginInfo>

/// Escaneia todos os diretórios padrão do sistema.
pub fn scan_system_vst3() -> Vec<Vst3PluginInfo>

pub struct Vst3PluginInfo {
    pub plugin_uid: [u8; 16],
    pub name: String,
    pub vendor: String,
    pub category: String,       // "Fx", "Instrument", etc.
    pub bundle_path: PathBuf,
    pub audio_inputs: u32,
    pub audio_outputs: u32,
    pub params: Vec<Vst3ParamInfo>,
}
```

### Binary path por plataforma

```rust
fn vst3_binary_path(bundle: &Path) -> PathBuf {
    let name = bundle.file_stem().unwrap();
    #[cfg(target_os = "macos")]
    return bundle.join("Contents/MacOS").join(name);
    #[cfg(target_os = "windows")]
    return bundle.join("Contents/x86_64-win").join(name).with_extension("vst3");
    #[cfg(target_os = "linux")]
    return bundle.join("Contents/x86_64-linux").join(name).with_extension("so");
}
```

### `processor.rs` — Wrapper Mono/Stereo

```rust
/// Mono: força processamento como mono (1 canal in/out)
pub struct Vst3Processor {
    plugin: Vst3Plugin,
    input_buf: Vec<f32>,
    output_buf: Vec<f32>,
}

impl MonoProcessor for Vst3Processor {
    fn process_block(&mut self, samples: &mut [f32]) { ... }
}

/// Stereo: 2 canais in/out
pub struct StereoVst3Processor {
    plugin: Vst3Plugin,
    input_l: Vec<f32>, input_r: Vec<f32>,
    output_l: Vec<f32>, output_r: Vec<f32>,
}

impl StereoProcessor for StereoVst3Processor {
    fn process_block(&mut self, frames: &mut [[f32; 2]]) { ... }
}
```

---

## Fase 2 — Bundled Open-Source VST3

### Plugins selecionados

Critérios: plugin ainda não temos em LV2, licença permissiva, qualidade comprovada, build multiplataforma viável.

| Plugin | Tipo | Bloco | Params mapeados | Licença | GitHub |
|--------|------|-------|-----------------|---------|--------|
| Cloud Seed | Reverb | `block-reverb` | decay, pre_delay, diffusion, mix | MIT | ValdemarOrn/CloudSeed |
| Cocoa Delay | Delay | `block-delay` | time_ms, feedback, ping_pong, mix | MIT | tesselode/cocoa-delay |
| Squeezer | Compressor | `block-dynamics` | threshold, ratio, attack, release, makeup, mix | GPL-3 | mzuther/Squeezer |
| CHOW Tape | Saturação/Tape | `block-gain` | drive, tone, mix | GPL-3 | jatinchowdhury18/AnalogTapeModel |
| modEQ | EQ Paramétrico | `block-filter` | 8 bandas: freq, gain, Q, type | GPL-3 | modularev/modEQ |
| Stone Mistress | Flanger | `block-modulation` | rate, depth, feedback, mix | GPL-3 | jpcima/stone-mistress |

> **Nota sobre GPL-3:** VST3 plugin GPL é permitido quando o host (OpenRig) é distribuído de forma que o usuário pode inspecionar e modificar o plugin. Verificar compatibilidade com a licença do OpenRig antes de bundlar.

### Estrutura de arquivos bundled

```
libs/vst3/
  macos-universal/
    CloudSeed.vst3/
    CocoaDelay.vst3/
    ...
  linux-x86_64/
    CloudSeed.vst3/
    ...
  windows-x64/
    CloudSeed.vst3/
    ...
```

### Model Definition (exemplo — Cloud Seed)

```rust
// crates/block-reverb/src/vst3_cloud_seed.rs

const MODEL_ID: &str = "vst3_cloud_seed";
const DISPLAY_NAME: &str = "Cloud Seed";
const PLUGIN_UID: [u8; 16] = [/* 16 bytes do classID */];
const BUNDLE_NAME: &str = "CloudSeed";  // nome sem extensão

// Param IDs descobertos via IEditController::getParameterInfo
const PARAM_DECAY:     u32 = 0;
const PARAM_PRE_DELAY: u32 = 1;
const PARAM_DIFFUSION: u32 = 2;
const PARAM_MIX:       u32 = 14;

fn schema() -> Result<ModelParameterSchema> { ... }
fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let bundle = resolve_vst3_bundle(BUNDLE_NAME)?;
    match layout {
        AudioChannelLayout::Stereo => {
            let mut plugin = Vst3Plugin::load(&bundle, &PLUGIN_UID, sample_rate as f64)?;
            plugin.set_param(PARAM_DECAY,     params.get_f32("decay")? as f64 / 100.0);
            plugin.set_param(PARAM_PRE_DELAY, params.get_f32("pre_delay")? / 100.0);
            plugin.set_param(PARAM_DIFFUSION, params.get_f32("diffusion")? as f64 / 100.0);
            plugin.set_param(PARAM_MIX,       params.get_f32("mix")? as f64 / 100.0);
            Ok(BlockProcessor::Stereo(Box::new(StereoVst3Processor::new(plugin))))
        }
        AudioChannelLayout::Mono => { ... }
    }
}

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "Valdemar Örndal",
    backend_kind: ReverbBackendKind::Vst3,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
```

### Path resolution (análoga ao LV2)

```rust
// crates/vst3/src/lib.rs
pub fn resolve_vst3_bundle(bundle_name: &str) -> Result<PathBuf> {
    // 1. Relative to executable
    // 2. From AssetPaths config (infra-filesystem)
    // 3. Error
}
```

Extensão do `AssetPaths` em `infra-filesystem`:
```rust
pub struct AssetPaths {
    pub lv2_libs: String,
    pub lv2_data: String,
    pub vst3_libs: String,    // novo: "libs/vst3/{platform}"
    pub nam_captures: String,
    pub ir_captures: String,
}
```

### BackendKind enum (cada crate afetada)

```rust
// Exemplo em block-reverb
pub enum ReverbBackendKind {
    Native,
    Ir,
    Vst3,   // novo
}
```

---

## Fase 3 — Dynamic VST3 (plugins do usuário)

### Novo bloco: `Vst3Dynamic`

```rust
// domain/src/lib.rs ou block-core
pub enum AudioBlockKind {
    // ... existentes ...
    Vst3Dynamic(Vst3DynamicBlock),
}

pub struct Vst3DynamicBlock {
    pub plugin_uid: String,        // hex string dos 16 bytes
    pub plugin_name: String,       // display name (do scan)
    pub bundle_path: String,       // path absoluto ao .vst3
    pub params: Vec<DynamicParam>,
}

pub struct DynamicParam {
    pub id: u32,
    pub value: f64,                // normalizado 0.0..1.0
}
```

### YAML persistence

```yaml
- type: vst3_dynamic
  enabled: true
  plugin_uid: "AABBCCDDEEFF00112233445566778899"
  plugin_name: "FabFilter Pro-Q 3"
  bundle_path: "/Library/Audio/Plug-Ins/VST3/FabFilter Pro-Q 3.vst3"
  params:
    - id: 1001
      value: 0.75
    - id: 1002
      value: 0.5
```

### Fluxo de descoberta

```
Abertura de projeto
  └─ scan_system_vst3() [async/background]
       └─ cache em memória: HashMap<uid, Vst3PluginInfo>

UI: "Adicionar bloco" → categoria "VST3"
  └─ listar plugins escaneados
       └─ usuário seleciona → cria Vst3DynamicBlock com params padrão

Edição de parâmetros
  └─ BlockEditor: detecta Vst3DynamicBlock
       └─ renderiza sliders dinâmicos a partir de DynamicParam
```

### Cache de scan

```rust
// crates/vst3/src/discovery.rs
thread_local! {
    static SCAN_CACHE: RefCell<Option<Vec<Vst3PluginInfo>>> = RefCell::new(None);
}

pub fn cached_scan() -> Vec<Vst3PluginInfo> {
    SCAN_CACHE.with(|cache| {
        let mut c = cache.borrow_mut();
        if c.is_none() {
            *c = Some(scan_system_vst3());
        }
        c.as_ref().unwrap().clone()
    })
}
```

O scan acontece apenas uma vez por sessão (lazy, na primeira vez que o usuário abre a lista de plugins VST3).

---

## Fase 4 — UI genérica de parâmetros

### BlockEditor — parâmetros dinâmicos

Para blocos `Vst3DynamicBlock`, o BlockEditor recebe uma lista de `DynamicParamItem` em vez de `BlockParameterItem` fixos:

```slint
struct DynamicParamItem {
    param_id: int,
    label: string,
    value: float,   // normalizado 0.0..1.0
    step_count: int // 0 = slider contínuo
}
```

O backend converte `DynamicParam` → `DynamicParamItem` antes de enviar para a UI. O slider renderiza o mesmo `knob` ou `slider` já existente, mas com bind para `on_dynamic_param_changed(param_id, value)`.

---

## Engine integration

```rust
// crates/engine/src/runtime.rs
fn build_audio_processor_for_model(block: &AudioBlock, ...) -> Result<BlockProcessor> {
    match &block.kind {
        // ... existentes ...
        AudioBlockKind::Vst3Dynamic(vst3) => {
            let bundle = PathBuf::from(&vst3.bundle_path);
            let uid = parse_uid(&vst3.plugin_uid)?;
            let mut plugin = Vst3Plugin::load(&bundle, &uid, sample_rate as f64)?;
            for p in &vst3.params {
                plugin.set_param(p.id, p.value);
            }
            match layout {
                Stereo => Ok(BlockProcessor::Stereo(Box::new(StereoVst3Processor::new(plugin)))),
                Mono   => Ok(BlockProcessor::Mono(Box::new(Vst3Processor::new(plugin)))),
            }
        }
    }
}
```

---

## Tratamento de erros

| Situação | Comportamento |
|----------|---------------|
| Plugin não encontrado no path | Log `WARN`, bloco em bypass |
| Plugin incompatível (32-bit, ARM mismatch) | Log `ERROR`, bloco em bypass |
| Plugin crasha em `process()` | Catch panic → bypass + log |
| UID não encontrado no bundle | Tentar primeiro plugin do factory |
| Parâmetro ID não existe | Ignorar silenciosamente |

---

## Plataformas e build

### Compilar bundled plugins

Cada plugin open-source precisa ser compilado como universal binary no macOS e para as arquiteturas alvo. Processo:

1. Fork/submodule do repo do plugin
2. Compilar com JUCE/DPF para VST3
3. Colocar em `libs/vst3/{platform}/`
4. Commitar o binário (como fazemos com LV2)

Alternativa: baixar releases oficiais quando disponíveis (Dragonfly, CloudSeed têm releases).

### `vst3-sys` license

O crate `vst3-sys` é MIT. O VST3 SDK da Steinberg tem licença dual (GPL-3 + comercial). Ao linkar dynamicamente (via `libloading`), evitamos incorporar o SDK no binário — o plugin já compilado contém o SDK. Verificar com advogado antes da distribuição comercial.

---

## Fases e dependências

```
Fase 1: crates/vst3 (host + discovery)
  └─ Fase 2: bundled plugins (depende de Fase 1)
  └─ Fase 3: dynamic loading (depende de Fase 1)
       └─ Fase 4: UI genérica (depende de Fase 3)
```

Fases 2 e 3 podem ser desenvolvidas em paralelo após Fase 1.

---

## Questões em aberto

1. **GPL-3 e distribuição**: plugins Squeezer, CHOW Tape, modEQ são GPL-3. Verificar se o modelo de distribuição do OpenRig é compatível.
2. **VST3 SDK license**: uso via `libloading` evita link estático — mas Steinberg pode ter restrições sobre hosting sem licença comercial.
3. **Scan assíncrono**: o scan pode ser lento (muitos plugins). Fazer em background thread com `std::thread` + `Arc<Mutex<>>` e notificar a UI quando completo.
4. **Salvar bundle_path**: paths mudam quando o usuário move plugins. Deve-se salvar também o `plugin_uid` e tentar re-encontrar o plugin por UID se o path falhar.
5. **VST3 preset files**: formato `.vstpreset` — suporte futuro, fora do escopo desta spec.
