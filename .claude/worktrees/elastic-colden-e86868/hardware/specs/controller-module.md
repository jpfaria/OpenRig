# Controller Module — Especificação

## Visão Geral

O Controller Module é o painel de controle do OpenRig. Contém footswitches para ativar/bypass efeitos e potenciômetros para ajustar parâmetros. Encaixa no Brain Frame via dovetail rail ou funciona standalone como controlador MIDI/HID.

## Dimensões

| Parâmetro | Valor | Notas |
|-----------|-------|-------|
| Comprimento | 500mm | Match com Brain Frame |
| Profundidade | 140mm | 2 rows FS + 1 row pots |
| Altura frontal | 25mm | Perfil mínimo (pedaleira) |
| Altura traseira | 35mm | Leve inclinação |
| Parede | 3mm | Reforçado na zona dos FS |
| Cantos | R=12mm | Match com Brain Frame |

## Layout dos Controles

```
┌────────────────────────────────────────────────────┐
│                 CONTROLLER MODULE                   │
│                                                     │
│  ●  ●  ●  ●  ●  ●  ●  ●  ●  ●   Row 1: FS      │  Y=40mm
│  LED indicators                                     │
│                                                     │
│  ●  ●  ●  ●  ●  ●  ●  ●  ●  ●   Row 2: FS      │  Y=90mm
│                                                     │
│  ◎  ◎  ◎  ◎  ◎  ◎  ◎  ◎  ◎  ◎   Row 3: Pots    │  Y=120mm
│                                                     │
└────────────────────────────────────────────────────┘
  ◄──────────────── 500mm ───────────────────────────►

  Espaçamento entre componentes: 46mm (center-to-center)
  X start: (500 - 9×46) / 2 = 43mm
```

## Componentes

### Footswitches (20 unidades)

| Parâmetro | Valor |
|-----------|-------|
| Tipo | Momentary, 12mm, SPST |
| Furo no painel | ∅12.5mm (12mm + 0.5 clearance) |
| Modelo sugerido | PBS-24B-4 ou similar |
| Disposição | 2 rows × 10, espaçamento 46mm |
| Fixação | Rosca + porca por baixo do painel |

### Potenciômetros (10 unidades)

| Parâmetro | Valor |
|-----------|-------|
| Tipo | 16mm, linear ou log, 10kΩ |
| Furo no painel | ∅7.5mm (7mm + 0.5 clearance) |
| Disposição | 1 row × 10, espaçamento 46mm |
| Knobs | ∅12mm, com indicador |
| Fixação | Rosca + porca |

### LEDs Indicadores (20 unidades)

| Parâmetro | Valor |
|-----------|-------|
| Tipo | LED 3mm ou WS2812B (RGB endereçável) |
| Furo no painel | ∅3.2mm (3mm + clearance) |
| Posição | 8mm acima de cada footswitch |
| Recomendação | WS2812B — cor programável por efeito |

### MCU (Microcontrolador)

| Parâmetro | Valor |
|-----------|-------|
| Opção 1 | RP2040 (Raspberry Pi Pico) |
| Opção 2 | STM32F103 (Blue Pill) |
| Protocolo | USB HID (botões + encoders) |
| Firmware | Custom (ler switches/pots, enviar via USB) |
| Alternativa | Usar GPIO do Orange Pi direto (sem MCU extra) |

## Reforço Estrutural (Zona dos Footswitches)

A área dos footswitches recebe pisadas com força (~50kg de impacto). O reforço consiste em:

- **Top panel espesso**: 4mm na zona dos FS (vs 3mm no resto)
- **Anéis reforçados**: ∅22mm × 5mm na parte de baixo do painel, em volta de cada furo
- **Pilares sólidos**: ∅16mm, do piso até o painel, sob cada footswitch
- **Costelas de conexão**: ribs de 3mm ligando os pilares na horizontal
- **Cross-ribs**: ribs diagonais conectando row 1 a row 2 (a cada 2 pilares)

```
  Top Panel
  ══════════════════  (4mm espessura)
      ╔══╗   ╔══╗     Anéis reforçados (22mm ∅)
      ║FS║   ║FS║     Footswitch holes
      ╚══╝   ╚══╝
       ┃      ┃       Pilares (16mm ∅)
       ┃──────┃       Costelas (3mm)
       ┃      ┃
  ════════════════════  Floor (3mm)
```

## Conexão com Brain Frame

- **Mecânica**: Dovetail rail ao longo da borda traseira (500mm)
- **Dados**: USB-C (15cm interno)
- **Fixação**: 4× M4 parafusos para travar
- **Alinhamento**: 2× dowel pins ∅6mm

## Impressão

| Peça | Dimensões máx | Tempo estimado |
|------|--------------|----------------|
| Left Bottom | 250 × 140mm | ~5h |
| Right Bottom | 250 × 140mm | ~5h |
| Left Top (reforçado) | 250 × 140mm | ~4h |
| Right Top (reforçado) | 250 × 140mm | ~4h |

## Modo Standalone

Sem o Brain Frame, o Controller funciona como:
- **Controlador MIDI USB**: cada switch/pot mapeado para CC/Note
- **HID Gamepad**: reconhecido como joystick genérico
- **OSC Controller**: via firmware customizado + WiFi (se usar ESP32)

Basta conectar o USB-C do Controller direto em um computador ou tablet.
