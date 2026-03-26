# OpenRig — Contexto do Projeto para Claude Code

## O que é o OpenRig

Pedalboard/rig virtual para guitarra em Rust. Processa áudio em cadeia (chain) com blocos (blocks) de efeitos e amplificadores. Tem interface gráfica em Slint.

---

## Fluxo de Desenvolvimento (OBRIGATORIO)

Toda mudanca segue este fluxo. Sem excecoes.

```
Issue → Branch → Commits → PR → Review/Merge
```

1. **Issue primeiro** — criar issue no GitHub antes de escrever qualquer codigo
2. **Branch por issue** — `git checkout -b issue-{N}-descricao-curta`
3. **Commits em ingles** — sem `Co-Authored-By`, foco no "why"
4. **PR para main** — `gh pr create` com `Closes #N` no body
5. **Merge policy**:
   - **Bugfix**: merge imediato apos criar o PR
   - **Feature**: PR aguarda review antes de merge

### Regras de codigo

- **Zero warnings** — `cargo build` nao pode ter nenhum warning
- **Zero acoplamento** — blocos nao referenciam modelos, brands ou effect types especificos
- **Single source of truth** — constantes definidas uma vez, nunca duplicadas
- **Separacao de concerns** — crates de business logic nao tem config visual/UI

Ver `CONTRIBUTING.md` para detalhes completos.

---

## Arquitetura

### Crates principais

- **`crates/block-preamp/`** — Bloco de pré-amplificador (preamp). Contém modelos NAM e nativos.
- **`crates/block-amp/`** — Bloco de amplificador completo (preamp + cab). Contém modelos nativos e NAM.
- **`crates/adapter-gui/`** — Interface gráfica em Slint (`.slint` files em `ui/`).
- **`crates/block-core/`** — Tipos base: `BlockProcessor`, `AudioChannelLayout`, `ParameterSet`, etc.
- **`crates/nam/`** — Integração com Neural Amp Modeler.
- **`crates/asset-runtime/`** — `EmbeddedAsset`, `materialize()` para assets compilados no binário.

### Assets

```
assets/
  brands/
    marshall/logo.svg   <- logo oficial worldvectorlogo, fill="currentColor"
    vox/logo.svg        <- logo oficial worldvectorlogo, cores #53ad99 (teal) + #d99346 (gold)
    native/             <- (vazio, sem marca real)
  amps/
    marshall/jcm800-2203/
      controls.svg      <- painel completo (fundo escuro, secoes, knobs como circulos)
      component.yaml    <- APENAS assets paths + svg_cx/cy dos controles
    native/
      american-clean/controls.svg + component.yaml
      brit-crunch/controls.svg + component.yaml
      modern-high-gain/controls.svg + component.yaml
    vox/ac30/
      controls.svg      <- padrao de referencia (AC30 e o template visual)
      amp.svg
      component.yaml    <- ainda tem brand/model/etc pois nao tem struct Rust
    generic/component.yaml
```

---

## Regras importantes

### Brand/type ficam no Rust, nao no YAML

`PreampModelDefinition` (em `crates/block-preamp/src/registry.rs`) tem:
```rust
pub struct PreampModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,   // ex: "Marshall JCM 800 2203"
    pub brand: &'static str,          // ex: "marshall", "vox", "native"
    pub backend_kind: PreampBackendKind,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub validate: fn(&ParameterSet) -> Result<()>,
    pub asset_summary: fn(&ParameterSet) -> Result<String>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
}
```

Os `component.yaml` so tem: caminhos de assets e posicoes SVG (`svg_cx`, `svg_cy`) dos controles — para uso futuro no overlay de knobs.

### Funcoes publicas em lib.rs

```rust
pub fn preamp_display_name(model: &str) -> Result<&'static str>
pub fn preamp_brand(model: &str) -> Result<&'static str>
pub fn preamp_type_label(model: &str) -> Result<&'static str>  // "native" | "NAM" | "IR"
```

### Logos de marcas

- **Sempre buscar em `cdn.worldvectorlogo.com`** — nao desenhar a mao.
- Remover o fundo branco/preto do SVG original.
- Marshall: usar `fill="currentColor"` para theming.
- Vox: cores fixas `#53ad99` e `#d99346` (ja sao coloridos).
- **Nao colocar a logo do brand dentro da imagem do equipamento.**

### Padrao de controls.svg (seguir o AC30)

`controls.svg` E o painel completo — nao criar `panel.svg` separado. Estrutura:
```svg
<svg viewBox="0 0 800 200" width="800" height="200">
  <!-- gradiente escuro de fundo -->
  <rect fill="url(#panel)"/>
  <!-- rotulo do modelo (esquerda) -->
  <!-- linha tracejada separando secoes -->
  <!-- rotulos de secao no topo -->
  <!-- circulos como ancoras de knob: fill="#111" stroke="#505050" stroke-width="1.5" -->
  <!-- texto de label abaixo de cada circulo -->
</svg>
```

Controles nao editaveis (ex: EQ fixo no Marshall NAM) -> mostrar com `opacity="0.6"`, sem `id`.
Controles editaveis -> tem `id="ctrl-xxx"` para overlay futuro.

### Registry auto-gerado (build.rs)

`crates/block-preamp/build.rs` escaneia `src/*.rs` procurando `MODEL_DEFINITION` e gera `generated_registry.rs` com array `MODEL_DEFINITIONS`. Ao criar novo modelo, basta criar o `.rs` com `pub const MODEL_DEFINITION: PreampModelDefinition = ...`.

---

## Modelos de preamp existentes

| ID | Display Name | Brand | Backend |
|----|-------------|-------|---------|
| `american_clean` | American Clean | native | Native |
| `brit_crunch` | Brit Crunch | native | Native |
| `modern_high_gain` | Modern High Gain | native | Native |
| `marshall_jcm_800_2203` | Marshall JCM 800 2203 | marshall | NAM |

Os 3 nativos usam `native_core::model_schema()` -> mesmos parametros: gain, bass, middle, treble, presence, depth, sag, master, bright.

Marshall NAM -> parametros: volume (50-70%), gain (10-100%), em steps de 10 (mapeado para captures .nam).

---

## Interface Grafica (Slint)

### Arquivos principais
- `crates/adapter-gui/ui/app-window.slint` — janela principal, 520px width para o BlockEditorWindow
- `crates/adapter-gui/ui/pages/project_chains.slint` — pagina de chains, contem `BlockEditorPanel`

### BlockEditorPanel — redesign do editor de blocos

Quando o bloco selecionado e `preamp`, mostrar a imagem do painel (`controls.svg`) em vez de so sliders.

Propriedades computadas:
```slint
property <bool> is-preamp:
    root.block-drawer-selected-type-index >= 0
    && root.block-drawer-selected-type-index < root.block-type-options.length
    && root.block-type-options[root.block-drawer-selected-type-index].icon_kind == "preamp";

property <string> selected-model-id:
    root.block-drawer-selected-model-index >= 0
    && root.block-drawer-selected-model-index < root.block-model-options.length
    ? root.block-model-options[root.block-drawer-selected-model-index].model_id
    : "";
```

Imagem do painel (ternary chain porque `@image-url()` precisa ser compile-time):
```slint
if root.is-preamp : Rectangle {
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

## Tipos de Instrumento

Cada chain tem um `instrument` que filtra quais blocos podem ser adicionados.

### Valores validos

`electric_guitar` | `acoustic_guitar` | `bass` | `voice` | `keys` | `drums` | `generic`

`generic` = sem filtragem, mostra todos os blocos.

### Constantes (em `crates/block-core/src/lib.rs`)

```rust
pub const INST_ELECTRIC_GUITAR: &str = "electric_guitar";
pub const INST_ACOUSTIC_GUITAR: &str = "acoustic_guitar";
pub const INST_BASS:             &str = "bass";
pub const INST_VOICE:            &str = "voice";
pub const INST_KEYS:             &str = "keys";
pub const INST_DRUMS:            &str = "drums";

pub const ALL_INSTRUMENTS:        &[&str] = &[/* todos acima */];
pub const GUITAR_BASS:            &[&str] = &[INST_ELECTRIC_GUITAR, INST_BASS];
pub const GUITAR_ACOUSTIC_BASS:   &[&str] = &[INST_ELECTRIC_GUITAR, INST_ACOUSTIC_GUITAR, INST_BASS];
```

### Suporte por modelo

Cada `MODEL_DEFINITION` declara `supported_instruments: &[&str]`. O `adapter-gui` filtra a lista de blocos disponiveis usando esse campo ao adicionar blocos a uma chain.

Exemplos de uso tipico:
- Preamps/amps: `GUITAR_ACOUSTIC_BASS`
- Amps/cabs com distorcao: `GUITAR_BASS`
- Efeitos universais (reverb, delay): `ALL_INSTRUMENTS`

### Persistencia

O campo `instrument` e salvo no YAML da chain. Valor padrao (retrocompatibilidade): `electric_guitar`. O instrumento e definido na criacao da chain e nao pode ser alterado depois.

```yaml
chains:
  - description: guitar 1
    instrument: electric_guitar
    # ...
```

---

## Pendencias / Proximos passos

- [ ] **Overlay de knobs sobre controls.svg** — usar `svg_cx`/`svg_cy` do component.yaml para posicionar componentes Slint interativos por cima da imagem
- [ ] **amp no BlockEditorPanel** — o Vox AC30 e `amp`, nao `preamp`; a logica `is-preamp` precisa de equivalente `is-amp`
- [ ] **Logo OpenRig** — `assets/brands/openrig/` esta vazio
- [ ] **output_db nos paineis nativos** — parametro existe no Rust mas nao esta no controls.svg
- [ ] **Vox AC30 -> struct Rust** — ainda nao tem `PreampModelDefinition` equivalente (e amp, arquitetura diferente)
- [ ] **Marshall JCM 800 2203 — versao Native** — criar modelo nativo com `NativeAmpHeadProfile` que expoe todos os controles reais: PRESENCE, BASS, MIDDLE, TREBLE, MASTER VOLUME, PRE-AMP. O painel (`controls.svg`) tera o painel completo igual ao amp real. O NAM atual (`marshall_jcm_800_2203`) continua com apenas MASTER + PRE-AMP.
