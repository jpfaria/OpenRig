# OpenRig — Contexto do Projeto para Claude Code

## O que é o OpenRig

Pedalboard/rig virtual para guitarra em Rust. Processa áudio em cadeia (chain) com blocos (blocks) de efeitos e amplificadores. Tem interface gráfica em Slint.

---

## Arquitetura

### Crates principais

- **`crates/block-amp-head/`** — Bloco de cabeçote de amplificador. Contém modelos NAM e nativos.
- **`crates/adapter-gui/`** — Interface gráfica em Slint (`.slint` files em `ui/`).
- **`crates/block-core/`** — Tipos base: `BlockProcessor`, `AudioChannelLayout`, `ParameterSet`, etc.
- **`crates/nam/`** — Integração com Neural Amp Modeler.
- **`crates/asset-runtime/`** — `EmbeddedAsset`, `materialize()` para assets compilados no binário.

### Assets

```
assets/
  brands/
    marshall/logo.svg   ← logo oficial worldvectorlogo, fill="currentColor"
    vox/logo.svg        ← logo oficial worldvectorlogo, cores #53ad99 (teal) + #d99346 (gold)
    native/             ← (vazio, sem marca real)
  amps/
    marshall/jcm800-2203/
      controls.svg      ← painel completo (fundo escuro, seções, knobs como círculos)
      component.yaml    ← APENAS assets paths + svg_cx/cy dos controles
    native/
      american-clean/controls.svg + component.yaml
      brit-crunch/controls.svg + component.yaml
      modern-high-gain/controls.svg + component.yaml
    vox/ac30/
      controls.svg      ← padrão de referência (AC30 é o template visual)
      amp.svg
      component.yaml    ← ainda tem brand/model/etc pois não tem struct Rust
    generic/component.yaml
```

---

## Regras importantes

### Brand/type ficam no Rust, não no YAML

`AmpHeadModelDefinition` (em `crates/block-amp-head/src/registry.rs`) tem:
```rust
pub struct AmpHeadModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,   // ex: "Marshall JCM 800 2203"
    pub brand: &'static str,          // ex: "marshall", "vox", "native"
    pub backend_kind: AmpHeadBackendKind,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub validate: fn(&ParameterSet) -> Result<()>,
    pub asset_summary: fn(&ParameterSet) -> Result<String>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
}
```

Os `component.yaml` só têm: caminhos de assets e posições SVG (`svg_cx`, `svg_cy`) dos controles — para uso futuro no overlay de knobs.

### Funções públicas em lib.rs

```rust
pub fn amp_head_display_name(model: &str) -> Result<&'static str>
pub fn amp_head_brand(model: &str) -> Result<&'static str>
pub fn amp_head_type_label(model: &str) -> Result<&'static str>  // "native" | "NAM" | "IR"
```

### Logos de marcas

- **Sempre buscar em `cdn.worldvectorlogo.com`** — não desenhar à mão.
- Remover o fundo branco/preto do SVG original.
- Marshall: usar `fill="currentColor"` para theming.
- Vox: cores fixas `#53ad99` e `#d99346` (já são coloridos).
- **Não colocar a logo do brand dentro da imagem do equipamento.**

### Padrão de controls.svg (seguir o AC30)

`controls.svg` É o painel completo — não criar `panel.svg` separado. Estrutura:
```svg
<svg viewBox="0 0 800 200" width="800" height="200">
  <!-- gradiente escuro de fundo -->
  <rect fill="url(#panel)"/>
  <!-- rótulo do modelo (esquerda) -->
  <!-- linha tracejada separando seções -->
  <!-- rótulos de seção no topo -->
  <!-- círculos como âncoras de knob: fill="#111" stroke="#505050" stroke-width="1.5" -->
  <!-- texto de label abaixo de cada círculo -->
</svg>
```

Controles não editáveis (ex: EQ fixo no Marshall NAM) → mostrar com `opacity="0.6"`, sem `id`.
Controles editáveis → têm `id="ctrl-xxx"` para overlay futuro.

### Registry auto-gerado (build.rs)

`crates/block-amp-head/build.rs` escaneia `src/*.rs` procurando `MODEL_DEFINITION` e gera `generated_registry.rs` com array `MODEL_DEFINITIONS`. Ao criar novo modelo, basta criar o `.rs` com `pub const MODEL_DEFINITION: AmpHeadModelDefinition = ...`.

---

## Modelos de amp existentes

| ID | Display Name | Brand | Backend |
|----|-------------|-------|---------|
| `american_clean` | American Clean | native | Native |
| `brit_crunch` | Brit Crunch | native | Native |
| `modern_high_gain` | Modern High Gain | native | Native |
| `marshall_jcm_800_2203` | Marshall JCM 800 2203 | marshall | NAM |

Os 3 nativos usam `native_core::model_schema()` → mesmos parâmetros: gain, bass, middle, treble, presence, depth, sag, master, bright.

Marshall NAM → parâmetros: volume (50–70%), gain (10–100%), em steps de 10 (mapeado para captures .nam).

---

## Interface Gráfica (Slint)

### Arquivos principais
- `crates/adapter-gui/ui/app-window.slint` — janela principal, 520px width para o BlockEditorWindow
- `crates/adapter-gui/ui/pages/project_chains.slint` — página de chains, contém `BlockEditorPanel`

### BlockEditorPanel — redesign do editor de blocos

Quando o bloco selecionado é `amp_head`, mostrar a imagem do painel (`controls.svg`) em vez de só sliders.

Propriedades computadas:
```slint
property <bool> is-amp-head:
    root.block-drawer-selected-type-index >= 0
    && root.block-drawer-selected-type-index < root.block-type-options.length
    && root.block-type-options[root.block-drawer-selected-type-index].icon_kind == "amp_head";

property <string> selected-model-id:
    root.block-drawer-selected-model-index >= 0
    && root.block-drawer-selected-model-index < root.block-model-options.length
    ? root.block-model-options[root.block-drawer-selected-model-index].model_id
    : "";
```

Imagem do painel (ternary chain porque `@image-url()` precisa ser compile-time):
```slint
if root.is-amp-head : Rectangle {
    x: 8px; y: 132px;
    width: parent.width - 16px;
    height: (parent.width - 16px) / 4;  // aspect ratio 4:1
    border-radius: 6px; clip: true;
    Image {
        source: root.selected-model-id == "marshall_jcm_800_2203"
            ? @image-url("caminho/marshall/jcm800-2203/controls.svg")
            : root.selected-model-id == "american_clean"
            ? @image-url("caminho/native/american-clean/controls.svg")
            : ...
            : @image-url("caminho/generic/controls.svg");
        image-fit: fill;
    }
}
```

---

## Pendências / Próximos passos

- [ ] **Overlay de knobs sobre controls.svg** — usar `svg_cx`/`svg_cy` do component.yaml para posicionar componentes Slint interativos por cima da imagem
- [ ] **amp_combo no BlockEditorPanel** — o Vox AC30 é `amp_combo`, não `amp_head`; a lógica `is-amp-head` precisa de equivalente `is-amp-combo`
- [ ] **Logo OpenRig** — `assets/brands/openrig/` está vazio
- [ ] **output_db nos painéis nativos** — parâmetro existe no Rust mas não está no controls.svg
- [ ] **Vox AC30 → struct Rust** — ainda não tem `AmpHeadModelDefinition` equivalente (é amp_combo, arquitetura diferente)
- [ ] **Marshall JCM 800 2203 — versão Native** — criar modelo nativo com `NativeAmpHeadProfile` que expõe todos os controles reais: PRESENCE, BASS, MIDDLE, TREBLE, MASTER VOLUME, PRE-AMP. O painel (`controls.svg`) terá o painel completo igual ao amp real. O NAM atual (`marshall_jcm_800_2203`) continua com apenas MASTER + PRE-AMP.
