# Carrier Plates — Display

## Bay: Display (180 × 130mm)

O bay de display fica na esquerda do Brain Frame, voltado para o topo (o display fica visível quando o pedalboard está no chão).

## Carriers Disponíveis

### Waveshare 7" HDMI (166.50 × 120.03mm)

| Parâmetro | Valor |
|-----------|-------|
| Arquivo | `carriers/display-waveshare7.scad` |
| Modelo | Waveshare 7" HDMI LCD (C) ou similar |
| PCB (OD) | 166.50 × 120.03mm |
| Área ativa | 157.45 × 89.90mm |
| Furos de montagem | 4× M2.5, espaçamento 156.63 × 115.04mm |
| Interface | HDMI + USB touch |
| Resolução | 1024 × 600 |

**Cutout no painel top:**

```
┌──────────────────────────────────────┐
│          ┌──────────────────┐        │
│          │                  │        │
│          │   Área ativa     │        │
│          │  157.45 × 89.90  │        │
│          │                  │        │
│          └──────────────────┘        │
│                                      │
│    ○                            ○    │  ← Furos M2.5 (156.63 × 115.04mm)
│                                      │
│    ○                            ○    │
│                                      │
│   PCB: 166.50 × 120.03mm            │
└──────────────────────────────────────┘
```

**Montagem:**
- Display encaixa por baixo do painel top
- Bezel recess de 2mm no top panel (vidro fica flush)
- Carrier plate segura o display por baixo com os 4 parafusos M2.5
- Cabo HDMI + USB passam por canaleta para o bay do SBC

### RPi 7" Official Touchscreen (Futuro)

| Parâmetro | Valor |
|-----------|-------|
| Arquivo | `carriers/display-rpi7.scad` |
| PCB | 194 × 110mm |
| Área ativa | 154.08 × 85.92mm |
| Interface | DSI (ribbon cable) |
| Status | Placeholder — medir quando disponível |

### 5" HDMI Generic (Futuro)

| Parâmetro | Valor |
|-----------|-------|
| Arquivo | `carriers/display-5inch.scad` |
| Status | Placeholder — vários modelos no mercado |

## Notas de Design

- O cutout da área ativa é feito no **top panel** do Brain Frame (não na carrier)
- O top panel tem um **bezel recess** de 2mm × (área ativa + 4mm cada lado)
- A carrier segura o PCB do display por baixo
- Cabos passam pela lateral da carrier até o bay do SBC
