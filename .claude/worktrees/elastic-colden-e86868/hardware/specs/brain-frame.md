# Brain Frame — Especificação

## Visão Geral

O Brain Frame é o chassis universal do OpenRig. Contém 3 bays padronizados onde encaixam as carrier plates dos componentes. Imprime em 4 peças (2 metades × top/bottom).

## Dimensões

| Parâmetro | Valor | Notas |
|-----------|-------|-------|
| Comprimento | 500mm | Acomoda 3 bays + paredes internas |
| Profundidade | 150mm | Display bay (130) + margem |
| Altura frontal | 30mm | Perfil baixo na borda de uso |
| Altura traseira | 55mm | Espaço para conectores |
| Parede externa | 3mm | PETG, 4 perimeters |
| Parede interna (entre bays) | 3mm | Divisória estrutural |
| Cantos externos | R=12mm | Arredondado |
| Piso | 3mm | Com furos de ventilação |

## Layout dos Bays

```
┌──────────────────────────────────────────────────────┐
│                     BRAIN FRAME                       │
│  ┌──────────────┐  ┌──────────┐  ┌───────────────┐  │
│  │  Display Bay │  │ SBC Bay  │  │  Audio Bay    │  │
│  │  180 × 130   │  │ 120×100  │  │  190 × 110   │  │
│  │              │  │          │  │               │  │
│  └──────────────┘  └──────────┘  └───────────────┘  │
│                                                       │
│  ◄──────────────────── 500mm ────────────────────►   │
│                                                       │
│  [PWR] [MIDI]        [USB-C]     [I/O via carriers]  │
│  ◄──── Painel Traseiro ────────────────────────────►  │
└──────────────────────────────────────────────────────┘
         ▲
    150mm │
         ▼
```

## Painel Traseiro

Portas fixas (cortadas no frame — não mudam):

| Porta | Posição (da esquerda) | Cutout | Notas |
|-------|----------------------|--------|-------|
| DC Barrel Jack | 30mm | ∅8mm | 5V/4A ou 12V/3A |
| MIDI DIN | 70mm | ∅16mm | 5-pin DIN, opcional |

Portas variáveis (cortadas nas carrier plates — I/O shield):

| Bay | Posição | Conteúdo |
|-----|---------|----------|
| SBC | Centro-esquerda | USB, HDMI, ETH do SBC |
| Audio | Direita | OUT L/R, USB, Phones da interface |

## Estrutura Interna

- **Divisórias** entre os bays (paredes internas de 3mm)
- **Canaletas de cabos** no piso (sulcos de 10×5mm) para routing de HDMI, USB, power
- **Furos de ventilação** no piso sob cada bay (grid de furos ∅3mm)
- **Dovetail rail** na borda frontal inferior (encaixe com Controller Module)

## Impressão

O frame imprime em **4 peças**:

| Peça | Dimensões máx | Tempo estimado |
|------|--------------|----------------|
| Left Bottom | 250 × 150mm | ~6h |
| Right Bottom | 250 × 150mm | ~6h |
| Left Top | 250 × 150mm | ~4h |
| Right Top | 250 × 150mm | ~4h |

- **Junta left/right**: lip de 2mm com parafusos M4 no centro
- **Junta top/bottom**: lip + parafusos M3 countersunk pelo topo (~16 parafusos)

## Pés de Borracha

- **6 pés** (4 cantos + 2 centro)
- **∅15mm**, recess de 2mm na base
- **Adesivos de borracha** padrão (pads autoadesivos)
