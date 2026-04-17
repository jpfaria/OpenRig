# Carrier Plates — Audio Interface

## Bay: Audio (190 × 110mm)

O bay de áudio fica na direita do Brain Frame. A interface de áudio é o coração do processamento — converte o sinal analógico da guitarra para digital e de volta.

## Carriers Disponíveis

### Focusrite Scarlett 2i2 (3rd/4th Gen)

| Parâmetro | Valor |
|-----------|-------|
| Arquivo | `carriers/audio-scarlett2i2.scad` |
| Dimensões | 175 × 99 × 47mm |
| Peso | ~340g |
| Entradas | 2× Combo XLR/TRS (frente) |
| Saídas | 2× TRS 1/4" (traseira) |
| Conexão | USB-C |
| Montagem | Apoio por base + velcro ou brackets laterais |

**Layout de portas:**

```
FRENTE (voltada pro usuário):
┌────────────────────────────────────┐
│  (●) Input 1    (●) Input 2       │  ← Combo XLR/TRS
│   ○ Gain 1       ○ Gain 2        │  ← Knobs
│        48V    INST    AIR         │  ← Switches
│         [GAIN HALO INDICATORS]    │
└────────────────────────────────────┘

TRASEIRA (voltada pro painel do Brain):
┌────────────────────────────────────┐
│  ◉ OUT L   ◉ OUT R   ▬ USB-C     │
│  ◉ Phones  ○ Volume              │
└────────────────────────────────────┘
```

**Montagem no carrier:**
- A Scarlett tem emborrachados na base (sem furos de montagem)
- Opções de fixação:
  1. **Brackets laterais impressos** que abraçam os lados (recommended)
  2. **Velcro industrial** na base (simples mas removível)
  3. **Strap + parafuso** por cima (seguro mas feio)
- Portas traseiras acessíveis via I/O shield recortado
- Portas frontais (inputs) acessíveis pelo top panel ou pela lateral

**I/O Shield (borda traseira):**

```
┌─────────────────────────────────────┐
│                                      │
│  ┌──────┐  ┌──────┐  ┌──────────┐  │
│  │OUT L │  │OUT R │  │  USB-C   │  │
│  │TRS¼" │  │TRS¼" │  │          │  │
│  └──────┘  └──────┘  └──────────┘  │
│                                      │
│  ┌──────────┐  ┌──────┐            │
│  │ Phones   │  │Volume│            │
│  │  ¼"      │  │      │            │
│  └──────────┘  └──────┘            │
│                                      │
└─────────────────────────────────────┘
```

**Considerações:**
- Os knobs de Gain da Scarlett precisam ficar acessíveis (frente ou top)
- Se necessário, os knobs de gain ficam embaixo do top panel com cutouts
- O switch 48V/INST/AIR pode ser acessado abrindo o top panel
- O USB-C da Scarlett conecta internamente no OPi 5B

### Teyun Q-26

| Parâmetro | Valor |
|-----------|-------|
| Arquivo | `carriers/audio-teyun-q26.scad` |
| Dimensões | **A MEDIR** |
| Entradas | TBD |
| Saídas | TBD |
| Conexão | USB |
| Status | Aguardando medições do João |

**TODO:** Medir comprimento × largura × altura, posição das portas, e definir método de fixação.

## Template para Nova Carrier de Áudio

```openscad
// === Carrier Audio: [MODELO] ===
include <../scad/lib/carrier-base.scad>

// Bay padrão (NÃO ALTERAR)
bay_w = 190;    // mm
bay_d = 110;    // mm

// Hardware específico (ALTERAR)
device_w = 175;       // mm - largura
device_d = 99;        // mm - profundidade
device_h = 47;        // mm - altura
bracket_thick = 3;    // mm - espessura dos brackets laterais
bracket_clearance = 0.5;  // mm - folga

// I/O Shield (posição das portas na traseira)
io_cutouts = [
    [-40, 5, 12, 12],   // OUT L (TRS)
    [-15, 5, 12, 12],   // OUT R (TRS)
    [20, 8, 12, 7],     // USB-C
    [-55, 5, 12, 12],   // Phones
];

carrier_audio(bay_w, bay_d, device_w, device_d, device_h,
              bracket_thick, bracket_clearance, io_cutouts);
```

## Notas de Design

- Interfaces de áudio geralmente não têm furos de montagem — usar brackets ou berço
- A base da carrier deve ter borracha/espuma para isolamento de vibração
- Prever circulação de ar (a Scarlett esquenta moderadamente)
- O gain da Scarlett é controlado por knobs físicos — avaliar se controla via software (Scarlett Control) ou se deixa acesso físico
