# Carrier Plates — SBC

## Bay: SBC (120 × 100mm)

O bay de SBC fica no centro do Brain Frame. Aceita qualquer Single Board Computer via carrier plate.

## Carriers Disponíveis

### Orange Pi 5B

| Parâmetro | Valor |
|-----------|-------|
| Arquivo | `carriers/sbc-opi5b.scad` |
| PCB | 90 × 64mm |
| SoC | Rockchip RK3588S |
| RAM | 4/8/16GB LPDDR4 |
| Armazenamento | eMMC onboard + microSD |
| WiFi | Wi-Fi 6 + BT 5.3 integrado |
| Furos de montagem | 4× M2.5, espaçamento ~82 × 56mm |
| Standoff height | 8mm |
| Orientação | Portas viradas para o painel traseiro |

**I/O Shield (borda traseira):**

```
┌────────────────────────────────────────────────────┐
│                                                     │
│  ┌──────┐  ┌────────┐  ┌──────────┐  ┌──────────┐ │
│  │USB2.0│  │  HDMI  │  │ Ethernet │  │  USB 3.0 │ │
│  │14×14 │  │ 16×7   │  │  17×14   │  │  USB-C   │ │
│  │stacked│  │        │  │ Gigabit  │  │  8×5     │ │
│  └──────┘  └────────┘  └──────────┘  └──────────┘ │
│                                                     │
└────────────────────────────────────────────────────┘
```

**Conexões internas:**
- HDMI → Display (cabo HDMI curto ou via DSI adapter)
- USB → Audio Interface (cabo USB-A para USB-B/C)
- USB-C → Power (5V/4A)
- GPIO → Controller MCU (opcional, para controle direto)

### Orange Pi 5 Plus

| Parâmetro | Valor |
|-----------|-------|
| Arquivo | `carriers/sbc-opi5plus.scad` |
| PCB | ~100 × 75mm |
| SoC | Rockchip RK3588 (full) |
| RAM | 4/8/16GB LPDDR4X |
| Extras | 2× 2.5G Ethernet, M.2 NVMe, HDMI IN |
| Furos de montagem | 4× M2.5, espaçamento ~92 × 67mm |
| Standoff height | 8mm |

**I/O Shield (borda traseira):**

```
┌────────────────────────────────────────────────────────────┐
│                                                             │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌────────┐ │
│  │USB2.0│ │USB2.0│ │HDMI  │ │HDMI  │ │HDMI  │ │2×2.5G  │ │
│  │      │ │      │ │OUT 1 │ │OUT 2 │ │ IN   │ │Ethernet│ │
│  └──────┘ └──────┘ └──────┘ └──────┘ └──────┘ └────────┘ │
│                                                             │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐                      │
│  │USB3.0│ │USB3.0│ │Type-C│ │Audio │                      │
│  │      │ │      │ │DP+PWR│ │ 3.5  │                      │
│  └──────┘ └──────┘ └──────┘ └──────┘                      │
│                                                             │
└────────────────────────────────────────────────────────────┘
```

### Orange Pi 6 Plus (Futuro)

| Parâmetro | Valor |
|-----------|-------|
| Arquivo | `carriers/sbc-opi6plus.scad` |
| PCB | TBD (aguardando lançamento) |
| Status | Placeholder — medir quando disponível |

### Raspberry Pi CM4 + IO Board (Futuro)

| Parâmetro | Valor |
|-----------|-------|
| Arquivo | `carriers/sbc-rpicm4.scad` |
| Notas | Compatível com Elk Audio OS para DSP de baixa latência |
| Status | Placeholder |

## Template para Nova Carrier de SBC

```openscad
// === Carrier SBC: [MODELO] ===
// Carrier plate para [MODELO] no bay SBC padrão OpenRig

include <../scad/lib/carrier-base.scad>

// Bay padrão (NÃO ALTERAR)
bay_w = 120;    // mm
bay_d = 100;    // mm

// Hardware específico (ALTERAR PARA SEU SBC)
pcb_w = 90;          // mm - largura do PCB
pcb_d = 64;          // mm - profundidade do PCB
mount_w = 82;        // mm - espaçamento furos X
mount_d = 56;        // mm - espaçamento furos Y
mount_hole = 2.7;    // mm - M2.5 clearance
standoff_h = 8;      // mm - altura dos standoffs
standoff_d = 6;      // mm - diâmetro dos standoffs

// I/O Shield recortes (posição relativa ao centro do PCB)
// [x_offset, z_offset, width, height]
io_cutouts = [
    [-30, 2, 14, 14],  // USB 2.0 stacked
    [-10, 3, 16, 7],   // HDMI
    [12, 2, 17, 14],   // Ethernet
    [25, 3, 8, 5],     // USB 3.0
];

carrier_sbc(
    bay_w, bay_d,
    pcb_w, pcb_d,
    mount_w, mount_d,
    mount_hole, standoff_h, standoff_d,
    io_cutouts
);
```
