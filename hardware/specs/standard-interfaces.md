# Interfaces Padrão — OpenRig Hardware

## Bay Standard

Todos os bays no Brain Frame seguem esta especificação. A interface mecânica **nunca muda** — é o contrato entre o frame e as carrier plates.

### Furação Padrão

```
    ●───────────────●───────────────●
    │               │               │
    │   Bay Area    │               │
    │  (variável)   │               │
    │               │               │
    ●               │               ●
    │               │               │
    │               │               │
    │               │               │
    ●───────────────●───────────────●
```

- **8 furos M3** (tap hole 2.5mm para rosca direta, ou 4.0mm para heat-set insert)
- **4 nos cantos**: inset 6mm das bordas do bay
- **4 intermediários**: centrados em cada aresta
- **Tolerância**: ±0.2mm

### Lip de Apoio

- **Largura**: 2mm ao redor do perímetro interno do bay
- **A carrier plate assenta sobre o lip**, flush com a superfície do frame
- **Profundidade do bay abaixo do lip**: 40mm (clearance)

### Bay Sizes

| Bay | Largura (mm) | Profundidade (mm) | Uso |
|-----|-------------|-------------------|-----|
| Display | 180 | 130 | Telas 5"-7" |
| SBC | 120 | 100 | Single Board Computers |
| Audio | 190 | 110 | Interfaces de áudio USB |

## Dovetail Rail (Brain ↔ Controller)

### Perfil do Trilho

```
         8mm
    ├──────────┤
    \          /  ← 60° angle
     \________/   ← 4mm base
      12mm top
```

- **Tipo**: trapezoidal (dovetail)
- **Comprimento**: total do Brain Frame (~500mm)
- **Ângulo**: 60° (padrão dovetail)
- **Largura topo**: 12mm
- **Largura base**: 8mm
- **Profundidade**: 8mm
- **Clearance**: 0.3mm por lado (para encaixe suave)

### Fixação

- **4 parafusos M4** distribuídos ao longo do rail
- **2 dowel pins** de 6mm (∅6.2mm nos furos) nas extremidades para alinhamento
- O dovetail **desliza lateralmente** para encaixar, depois trava com parafusos

### Conexão Elétrica

- **USB-C interno** entre Brain e Controller
- **Cabo**: 15cm, USB 2.0 suficiente (HID data)
- **Passagem de cabo**: slot 15×5mm na interface entre os módulos

## Carrier Plate Standard

### Estrutura

```
┌─────────────────────────────────────┐
│  ○    ○         ○         ○    ○   │ ← Furos M3 (match bay)
│                                     │
│    ┌─────────────────────────┐     │
│    │                         │     │
│    │   Standoffs específicos │     │
│    │   do hardware           │     │
│    │                         │     │
│    └─────────────────────────┘     │
│                                     │
│  ○    ○         ○         ○    ○   │
│▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓│ ← I/O Shield (borda traseira)
└─────────────────────────────────────┘
```

### Especificações

| Parâmetro | Valor |
|-----------|-------|
| Espessura | 4-5mm |
| Material | PETG ou PLA+ |
| Infill | 20-30% |
| Furos perimetrais | M3 passante (3.2mm) |
| Countersink | M3 cabeça chata (6mm ∅, 2mm profundidade) |
| I/O Shield | Recortes na borda traseira, específicos do modelo |

### Convenção de Nomenclatura

```
carrier-{tipo}-{modelo}.scad

Exemplos:
  carrier-sbc-opi5b.scad
  carrier-sbc-opi5plus.scad
  carrier-display-waveshare7.scad
  carrier-audio-scarlett2i2.scad
```

## Parafusos Usados

| Local | Tipo | Tamanho | Quantidade |
|-------|------|---------|-----------|
| Carrier → Bay | M3 × 8mm | Cabeça chata (countersunk) | 8 por bay |
| SBC → Carrier | M2.5 × 6mm | Cabeça panela | 4 por SBC |
| Display → Carrier | M2.5 × 6mm | Cabeça panela | 4 por display |
| Audio → Carrier | M3 × 10mm | Cabeça panela | 4 por interface |
| Top → Bottom (frame) | M3 × 10mm | Countersunk | ~20 |
| Brain → Controller | M4 × 12mm | Cabeça panela | 4 |
| Dowel pins | ∅6 × 20mm | Aço | 2 |

## Impressão 3D — Configurações Recomendadas

| Peça | Layer | Infill | Paredes | Suporte | Tempo estimado |
|------|-------|--------|---------|---------|----------------|
| Brain Frame (metade) | 0.2mm | 40% | 4 | Não | ~8h |
| Controller (metade) | 0.2mm | 40% | 4 | Não | ~6h |
| Carrier Plate | 0.2mm | 20% | 3 | Não | ~30min |
| Top Panel (com reforço) | 0.2mm | 50% | 4 | Não | ~5h |
