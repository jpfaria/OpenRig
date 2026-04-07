# EQ Widgets Design — Issue #156

## Objetivo

Adicionar dois componentes Slint de EQ interativos (`ParametricEqControl` e `GraphicEqControl`) e redesenhar o Three Band EQ com parâmetros reais de shelf/peak.

---

## Modelos afetados

| Modelo | Widget |
|--------|--------|
| `eq_three_band` (substitui `eq_three_band_basic`) | `ParametricEqControl` |
| `lv2_zameq2` | `ParametricEqControl` |
| `lv2_tap_equalizer` | `ParametricEqControl` |
| `lv2_tap_equalizer_bw` | `ParametricEqControl` |
| `lv2_zamgeq31` | `GraphicEqControl` |

---

## Decisões de design

### Three Band EQ — substituição direta

- O modelo `eq_three_band_basic` é **substituído** por `eq_three_band`.
- Mesmo ID pode ser mantido (`eq_three_band`) — projetos existentes com `eq_three_band_basic` precisarão ser atualizados manualmente (aceitável: produto em desenvolvimento).
- Novos parâmetros:
  - `low_gain` (-12..+12 dB, step 0.1), `low_freq` (80..320 Hz, step 1)
  - `mid_gain` (-12..+12 dB, step 0.1), `mid_freq` (200..5000 Hz, step 1), `mid_q` (0.1..6.0, step 0.01)
  - `high_gain` (-12..+12 dB, step 0.1), `high_freq` (3000..12000 Hz, step 1)
- DSP: Low shelf (biquad), Peak (biquad), High shelf (biquad). Implementado em Rust nativo.
- Defaults: low_gain=0, low_freq=200, mid_gain=0, mid_freq=1000, mid_q=1.0, high_gain=0, high_freq=8000.

### Curva de frequência — gerada no Rust via callback

- O Slint envia os parâmetros atuais para o Rust via callback `compute-eq-curve(bands) → string`.
- O Rust calcula os pontos da curva de resposta real (usando coeficientes biquad/shelf) em ~200 pontos no range 20Hz–20kHz (escala log).
- Retorna uma string SVG path: `"M x0,y0 L x1,y1 L x2,y2 ..."`.
- O Slint usa `Path { commands: <path-data>; ... }` para desenhar a curva total (branca) e curvas individuais por banda (coloridas, semitransparentes).
- O callback é disparado toda vez que um parâmetro muda (drag de ponto ou handle).

**Alternativa descartada:** calcular a curva só no Slint com spline visual. Descartada porque: (a) a issue menciona isso como simplificação, mas com o Rust já conhecendo os coeficientes biquad, a curva real é mais correta e não tem custo significativo; (b) passar os parâmetros numéricos seria mais trabalhoso do que passar a curva pronta.

### Interatividade — drag direto no gráfico

- Cada banda é representada por um círculo colorido (ponto de controle) posicionado em `(freq, gain)` no espaço do gráfico.
- `TouchArea` sobre o ponto: drag vertical = gain, drag horizontal = freq (escala log).
- Para bandas com Q (peaks): dois "handles" de asa, posicionados lateralmente ao ponto central. Drag horizontal do handle = ajusta Q/bandwidth. Handle renderizado como pequenos retângulos ou círculos menores.
- Cores por banda: Array fixo — `[#4488ff, #ff8844, #44ff88, #ff4488, #ffcc44, ...]`.
- Curvas individuais: `opacity: 0.35`, curva total: `opacity: 1.0`, cor branca.

### GraphicEqControl

- 31 sliders verticais lado a lado (um por banda do ZamGEQ31).
- Range: -12 dB a +12 dB, centro = 0 dB.
- Curva spline conectando os topos dos sliders (aqui sim, visual/spline, já que são bandas fixas e a curva real seria idêntica à spline em EQ gráfico).
- Label de frequência no eixo X (apenas alguns: 100Hz, 1kHz, 10kHz).
- Master gain separado como knob acima.

---

## Mudanças no `ParameterWidget` (block-core)

```rust
pub enum ParameterWidget {
    Knob,
    Toggle,
    Select,
    FilePicker,
    TextInput,
    ParametricEq,   // novo
    GraphicEq,      // novo
}
```

- `ParametricEq` e `GraphicEq` são widgets de nível de bloco (não por parâmetro individual).
- O schema do modelo declara qual widget usar no campo `widget` do primeiro parâmetro do grupo... **não**: melhor adicionar um campo `widget_override: Option<ParameterWidget>` no `ModelParameterSchema` — um único widget que substitui toda a renderização de parâmetros do bloco.

```rust
pub struct ModelParameterSchema {
    // campos existentes ...
    pub block_widget: Option<ParameterWidget>, // None = renderização padrão por parâmetro
}
```

- No adapter-gui, se `block_widget == Some(ParametricEq)`, renderiza `ParametricEqControl` no lugar dos knobs/sliders individuais.
- Os parâmetros individuais continuam existindo no schema (para validação, persistência, e como fallback).

---

## Estrutura dos dados passados ao Slint

### Para ParametricEqControl

```slint
struct EqBand {
    id: string,          // "low", "mid", "high", "peak1", etc.
    kind: string,        // "low_shelf", "high_shelf", "peak", "notch"
    freq: float,         // Hz
    gain: float,         // dB
    q: float,            // 0.1–6.0 (ignorado para shelves)
    freq_min: float,
    freq_max: float,
    gain_min: float,
    gain_max: float,
    q_min: float,
    q_max: float,
    color: color,
}
```

Callback de saída: `band-changed(id: string, freq: float, gain: float, q: float)` → adapter-gui atualiza os parâmetros individuais.

Callback de curva: `compute-eq-curve(bands: [EqBand]) → string` (chamado pelo Slint, implementado no Rust).

### Para GraphicEqControl

```slint
struct GEqBand {
    index: int,
    freq_label: string,
    gain: float,    // -12..+12 dB
}
```

Callback: `geq-band-changed(index: int, gain: float)`.

---

## Arquivos a criar/modificar

### block-core
- `src/param.rs`: adicionar variantes `ParametricEq`, `GraphicEq` a `ParameterWidget`; adicionar `block_widget: Option<ParameterWidget>` a `ModelParameterSchema`

### block-filter
- `src/native_eq_three_band.rs`: novo arquivo, substitui `native_eq_three_band_basic.rs`
  - Parâmetros: low_gain, low_freq, mid_gain, mid_freq, mid_q, high_gain, high_freq
  - DSP: biquad low shelf + peak + high shelf
  - `block_widget: Some(ParameterWidget::ParametricEq)`
- `src/native_eq_three_band_basic.rs`: **remover** (ou manter como deprecated se necessário)
- `src/lv2_zameq2.rs`: adicionar `block_widget: Some(ParameterWidget::ParametricEq)`
- `src/lv2_tap_equalizer.rs`: idem
- `src/lv2_tap_equalizer_bw.rs`: idem
- `src/lv2_zamgeq31.rs`: adicionar `block_widget: Some(ParameterWidget::GraphicEq)`
- `src/registry.rs`: atualizar `FilterModelDefinition` para incluir `block_widget`

### adapter-gui
- `src/lib.rs`:
  - Adicionar struct `EqBand` e `GEqBand` (ou equivalentes no Slint)
  - Implementar callback `compute-eq-curve` (cálculo de resposta biquad em Rust)
  - Mapear `block_widget` para `widget_kind` no `BlockParameterItem` ou usar campo separado no `BlockEditorData`
  - Callback `band-changed` → atualiza parâmetros individuais no estado
  - Callback `geq-band-changed` → idem

### adapter-gui/ui
- `ui/models.slint`: adicionar structs `EqBand`, `GEqBand`
- `ui/pages/block_panel_editor.slint`:
  - Adicionar propriedades `eq-bands: [EqBand]`, `geq-bands: [GEqBand]`
  - Adicionar callbacks `compute-eq-curve`, `band-changed`, `geq-band-changed`
  - Renderizar `ParametricEqControl` quando `block_widget == "parametric_eq"`
  - Renderizar `GraphicEqControl` quando `block_widget == "graphic_eq"`
- `ui/pages/parametric_eq_control.slint`: novo componente
- `ui/pages/graphic_eq_control.slint`: novo componente

---

## Dimensões dos componentes

- `ParametricEqControl`: largura total do painel editor, altura ~200px
- Eixo X: 20Hz a 20kHz, escala logarítmica
- Eixo Y: -18dB a +18dB (margem além dos limites dos parâmetros)
- Fundo: gradiente escuro, grid de referência sutil (linhas em 100Hz, 1kHz, 10kHz e 0dB, ±6dB, ±12dB)

- `GraphicEqControl`: largura total, altura ~180px
- 31 sliders com ~8px de largura cada, gap de 2px

---

## Não incluído neste escopo

- Resposta de fase
- Botão bypass por banda
- Preset de curva
- Suporte a mais de 6 bandas parametricas no mesmo widget (ZamEQ2 tem 4 bandas, TAP tem 8)
