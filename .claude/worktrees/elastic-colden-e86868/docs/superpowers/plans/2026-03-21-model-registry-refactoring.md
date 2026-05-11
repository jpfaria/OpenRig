# Model Registry Refactoring — brand, display_name, backend_kind

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Todas as ModelDefinition structs passam a ter `display_name`, `brand` e `backend_kind`. Arquivos dos modulos renomeados com prefixo do tipo (nam_, native_, ir_). Informacao de brand/type centralizada no modulo, nunca inferida pelo adapter-gui.

**Architecture:** Cada crate de bloco (block-amp-head, block-amp-combo, block-cab, block-gain, block-full-rig) tem uma struct ModelDefinition no registry.rs. Adicionar os 3 campos faltantes. Renomear arquivos .rs dos modulos com prefixo. O build.rs descobre modulos automaticamente — renomear funciona sem mexer no build.rs. Expor funcoes publicas `brand()` e `type_label()` em cada crate. Remover funcoes `model_brand()` e `model_type_label()` do adapter-gui que inferem pelo model_id.

**Tech Stack:** Rust, Slint

---

## Mapeamento de Modulos

### block-amp-head (ja tem brand/display_name/backend_kind)
| Arquivo atual | Novo nome | brand | backend |
|---|---|---|---|
| `american_clean.rs` | `native_american_clean.rs` | `""` | Native |
| `brit_crunch.rs` | `native_brit_crunch.rs` | `""` | Native |
| `modern_high_gain.rs` | `native_modern_high_gain.rs` | `""` | Native |
| `marshall_jcm_800.rs` | `nam_marshall_jcm_800.rs` | `"marshall"` | Nam |

### block-amp-combo (falta brand/display_name/backend_kind)
| Arquivo atual | Novo nome | brand | backend |
|---|---|---|---|
| `blackface_clean.rs` | `native_blackface_clean.rs` | `""` | Native |
| `bogner_ecstasy.rs` | `nam_bogner_ecstasy.rs` | `"bogner"` | Nam |
| `chime.rs` | `native_chime.rs` | `""` | Native |
| `tweed_breakup.rs` | `native_tweed_breakup.rs` | `""` | Native |

### block-cab (tem backend_kind, falta brand/display_name)
| Arquivo atual | Novo nome | brand | backend |
|---|---|---|---|
| `american_2x12.rs` | `native_american_2x12.rs` | `""` | Native |
| `brit_4x12.rs` | `native_brit_4x12.rs` | `""` | Native |
| `marshall_4x12_v30.rs` | `ir_marshall_4x12_v30.rs` | `"marshall"` | Ir |
| `vintage_1x12.rs` | `native_vintage_1x12.rs` | `""` | Native |

### block-gain (falta tudo)
| Arquivo atual | Novo nome | brand | backend |
|---|---|---|---|
| `blues_overdrive_bd_2.rs` | `nam_boss_blues_overdrive_bd_2.rs` | `"boss"` | Nam |
| `ibanez_ts9.rs` | `nam_ibanez_ts9.rs` | `"ibanez"` | Nam |

### block-full-rig (falta tudo)
| Arquivo atual | Novo nome | brand | backend |
|---|---|---|---|
| `roland_jc_120b_jazz_chorus.rs` | `nam_roland_jc_120b_jazz_chorus.rs` | `"roland"` | Nam |

### Crates sem marca (delay, reverb, dyn, filter, wah, mod, ir, nam, util)
Estes crates nao tem marcas — sao todos nativos. Nao precisam de refactoring de brand. Podem receber display_name no futuro se necessario.

---

### Task 1: block-amp-head — renomear arquivos com prefixo

**Files:**
- Rename: `crates/block-amp-head/src/american_clean.rs` → `native_american_clean.rs`
- Rename: `crates/block-amp-head/src/brit_crunch.rs` → `native_brit_crunch.rs`
- Rename: `crates/block-amp-head/src/modern_high_gain.rs` → `native_modern_high_gain.rs`
- Rename: `crates/block-amp-head/src/marshall_jcm_800.rs` → `nam_marshall_jcm_800.rs`

- [ ] **Step 1: Renomear os 4 arquivos**

```bash
cd crates/block-amp-head/src
git mv american_clean.rs native_american_clean.rs
git mv brit_crunch.rs native_brit_crunch.rs
git mv modern_high_gain.rs native_modern_high_gain.rs
git mv marshall_jcm_800.rs nam_marshall_jcm_800.rs
```

- [ ] **Step 2: Atualizar lib.rs se houver refs explicitas**

Verificar se `lib.rs` tem `mod american_clean;` etc. Se sim, atualizar. Se nao (build.rs auto-descobre), nada a fazer.

- [ ] **Step 3: Compilar e testar**

```bash
cargo build -p block-amp-head
cargo test -p block-amp-head
```

- [ ] **Step 4: Commit**

```bash
git commit -m "refactor(block-amp-head): renomeia modulos com prefixo native_/nam_"
```

---

### Task 2: block-amp-combo — adicionar campos + renomear

**Files:**
- Modify: `crates/block-amp-combo/src/registry.rs` — adicionar `display_name`, `brand`, `backend_kind`
- Modify: `crates/block-amp-combo/src/lib.rs` — adicionar `AmpComboBackendKind` enum + funcoes publicas
- Modify: 4 modulos — adicionar campos ao MODEL_DEFINITION
- Rename: 4 arquivos com prefixo

- [ ] **Step 1: Adicionar BackendKind e campos na struct**

Em `registry.rs`, adicionar:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmpComboBackendKind { Native, Nam, Ir }

pub struct AmpComboModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub brand: &'static str,
    pub backend_kind: AmpComboBackendKind,
    // ... campos existentes
}
```

- [ ] **Step 2: Adicionar DISPLAY_NAME + campos nos 4 modulos**

Cada modulo recebe:
```rust
pub const DISPLAY_NAME: &str = "...";
pub const MODEL_DEFINITION: AmpComboModelDefinition = AmpComboModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "...",
    backend_kind: AmpComboBackendKind::...,
    // ... existentes
};
```

- [ ] **Step 3: Adicionar funcoes publicas em lib.rs**

```rust
pub fn amp_combo_brand(model: &str) -> Result<&'static str> {
    Ok(registry::find_model_definition(model)?.brand)
}
pub fn amp_combo_type_label(model: &str) -> Result<&'static str> {
    // ... baseado em backend_kind
}
```

- [ ] **Step 4: Renomear arquivos**

```bash
cd crates/block-amp-combo/src
git mv blackface_clean.rs native_blackface_clean.rs
git mv bogner_ecstasy.rs nam_bogner_ecstasy.rs
git mv chime.rs native_chime.rs
git mv tweed_breakup.rs native_tweed_breakup.rs
```

- [ ] **Step 5: Compilar e testar**

```bash
cargo build -p block-amp-combo
cargo test -p block-amp-combo
```

- [ ] **Step 6: Commit**

```bash
git commit -m "refactor(block-amp-combo): adiciona brand/display_name/backend, renomeia com prefixo"
```

---

### Task 3: block-cab — adicionar brand/display_name + renomear

**Files:**
- Modify: `crates/block-cab/src/registry.rs` — adicionar `display_name`, `brand`
- Modify: 4 modulos
- Rename: 4 arquivos

- [ ] **Step 1: Adicionar campos na struct CabModelDefinition**

```rust
pub struct CabModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub brand: &'static str,
    pub backend_kind: CabBackendKind,
    // ... existentes
}
```

- [ ] **Step 2: Atualizar os 4 modulos com DISPLAY_NAME e brand**

- [ ] **Step 3: Renomear arquivos**

```bash
cd crates/block-cab/src
git mv american_2x12.rs native_american_2x12.rs
git mv brit_4x12.rs native_brit_4x12.rs
git mv marshall_4x12_v30.rs ir_marshall_4x12_v30.rs
git mv vintage_1x12.rs native_vintage_1x12.rs
```

- [ ] **Step 4: Compilar, testar, commit**

---

### Task 4: block-gain — adicionar campos + renomear

**Files:**
- Modify: `crates/block-gain/src/registry.rs`
- Modify: 2 modulos
- Rename: 2 arquivos

- [ ] **Step 1: Adicionar BackendKind, display_name, brand na struct**

- [ ] **Step 2: Atualizar modulos**

| Modulo | DISPLAY_NAME | brand | backend |
|---|---|---|---|
| `blues_overdrive_bd_2.rs` | `"Blues Overdrive BD-2"` | `"boss"` | Nam |
| `ibanez_ts9.rs` | `"Ibanez TS9"` | `"ibanez"` | Nam |

- [ ] **Step 3: Renomear**

```bash
git mv blues_overdrive_bd_2.rs nam_boss_blues_overdrive_bd_2.rs
git mv ibanez_ts9.rs nam_ibanez_ts9.rs
```

- [ ] **Step 4: Compilar, testar, commit**

---

### Task 5: block-full-rig — adicionar campos + renomear

**Files:**
- Modify: `crates/block-full-rig/src/registry.rs`
- Modify: 1 modulo
- Rename: 1 arquivo

- [ ] **Step 1: Adicionar campos na struct**

- [ ] **Step 2: Atualizar modulo roland_jc_120b**

| Modulo | DISPLAY_NAME | brand | backend |
|---|---|---|---|
| `roland_jc_120b_jazz_chorus.rs` | `"JC-120B Jazz Chorus"` | `"roland"` | Nam |

- [ ] **Step 3: Renomear**

```bash
git mv roland_jc_120b_jazz_chorus.rs nam_roland_jc_120b_jazz_chorus.rs
```

- [ ] **Step 4: Compilar, testar, commit**

---

### Task 6: Enriquecer BlockModelCatalogEntry com brand/type_label

**Files:**
- Modify: `crates/project/src/catalog.rs` — adicionar `brand` e `type_label` a `BlockModelCatalogEntry`
- Modify: crates que implementam `schema_for_block_model` para retornar brand/type_label
- Modify: `crates/adapter-gui/src/lib.rs` — usar dados do catalog em vez de inferir

- [ ] **Step 1: Adicionar campos a BlockModelCatalogEntry**

- [ ] **Step 2: Propagar brand/type_label pelo fluxo de dados**

- [ ] **Step 3: Remover model_brand() e model_type_label() do adapter-gui**

- [ ] **Step 4: Compilar tudo, testar, commit**

---

### Task 7: Limpar build.rs — atualizar excludes se necessario

Verificar se os `build.rs` de cada crate tem excludes (`matches!(stem, "lib" | "registry" | ...)`) que precisam ser atualizados apos renomear. O padrao atual (`native_core`) deve continuar excluido em block-amp-head.

- [ ] **Step 1: Verificar cada build.rs**
- [ ] **Step 2: Compilar tudo**

```bash
cargo build
cargo test
```

- [ ] **Step 3: Commit final**

```bash
git commit -m "refactor: model registry completo com brand/display_name/backend em todos os crates"
```
