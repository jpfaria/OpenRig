# Arquitetura de Hardware — OpenRig

## Visão Geral

O OpenRig usa uma arquitetura modular baseada em **bays padronizados** e **carrier plates intercambiáveis**. O conceito é inspirado em gabinetes de PC (onde o mesmo case aceita diferentes motherboards) e em pedalboards modulares (onde módulos encaixam e desencaixam).

## Princípios de Design

1. **Interface padrão**: Cada bay tem um padrão fixo de furos M3 que nunca muda
2. **Carrier plates**: Placas adaptadoras finas que fazem a ponte entre o bay e o hardware
3. **I/O Shield**: Cada carrier de SBC/audio tem um recorte de portas (I/O shield) específico do modelo
4. **Dovetail rail**: Módulos maiores (Brain + Controller) encaixam via trilho trapezoidal
5. **Dados via USB-C**: Conexão entre módulos sempre por USB-C padrão
6. **Print-friendly**: Toda peça cabe em mesa de 300×300mm, imprime em top+bottom

## Diagrama de Blocos

```
                    ┌─────────────────────────────┐
                    │        BRAIN FRAME           │
                    │                               │
  ┌─────────┐      │  ┌─────────┐  ┌───────────┐  │      ┌─────────────┐
  │ Display │◄─────┤  │   SBC   │  │   Audio   │  ├─────►│  Back Panel  │
  │  7"     │ HDMI │  │ OPi 5B  │  │ Scarlett  │  │      │  I/O Ports   │
  │         │ DSI  │  │         │  │   2i2     │  │      │             │
  └─────────┘      │  └────┬────┘  └─────┬─────┘  │      └─────────────┘
                    │       │USB          │USB      │
                    │       └──────┬──────┘         │
                    │              │                 │
                    └──────────────┼─────────────────┘
                                   │ USB-C
                    ┌──────────────┼─────────────────┐
                    │    CONTROLLER MODULE            │
                    │                                  │
                    │  ┌────────────────────────────┐  │
                    │  │ 10 Footswitches (Row 1)    │  │
                    │  ├────────────────────────────┤  │
                    │  │ 10 Footswitches (Row 2)    │  │
                    │  ├────────────────────────────┤  │
                    │  │ 10 Potentiometers          │  │
                    │  └────────────────────────────┘  │
                    │  MCU: STM32/RP2040 (USB HID)    │
                    └──────────────────────────────────┘
```

## Fluxo de Sinal

```
Guitarra ──► Audio IN (Scarlett) ──► USB ──► OPi 5B ──► OpenRig Software
                                                              │
                                                    DSP (NAM, LV2, IR)
                                                              │
             Amp/FRFR ◄── Audio OUT (Scarlett) ◄── USB ◄─────┘
                                                              │
                                              Display ◄── HDMI/DSI
                                                              │
                                           Controller ◄── USB (HID)
```

## Dimensões do Brain Frame

| Parâmetro | Valor | Notas |
|-----------|-------|-------|
| Comprimento total | ~500mm | Acomoda 3 bays + paredes |
| Profundidade | ~140mm | Maior bay (display) + margem |
| Altura frontal | 30mm | Perfil baixo |
| Altura traseira | 50mm | Espaço para portas |
| Parede | 3mm | PETG estrutural |
| Cantos | R=12mm | Arredondado |

## Padrão de Bay

Todos os bays usam o mesmo padrão de montagem:

- **8 furos M3** no perímetro (rosca direta no plástico ou heat-set insert)
- **Espaçamento**: 4 nos cantos + 4 intermediários
- **Profundidade do bay**: 40mm (clearance para componentes)
- **Borda de apoio**: lip de 2mm onde a carrier plate assenta

## Padrão de Carrier Plate

Cada carrier plate tem:

- **Espessura**: 4-5mm
- **8 furos M3 passantes** no perímetro (match com o bay)
- **Standoffs específicos** do hardware (M2.5 para SBCs, M3 para áudio)
- **I/O shield** recortado na borda traseira
- **Canaleta de cabos** para routing interno

## Encaixe Brain ↔ Controller

- **Dovetail rail**: trilho trapezoidal (ângulo 60°) ao longo da borda de 500mm
- **Profundidade do encaixe**: 8mm
- **4 parafusos M4** para travar (evita desencaixar sem querer)
- **Pinos de alinhamento**: 2 dowel pins de 6mm nas extremidades
- **Dados**: cabo USB-C curto interno entre os módulos

## Roadmap de Carriers

### Fase 1 (Agora)
- [x] Display Waveshare 7" (166.50 × 120.03mm)
- [x] SBC Orange Pi 5B (90 × 64mm)
- [ ] Audio Focusrite Scarlett 2i2 (175 × 99mm)
- [ ] Audio Teyun Q-26 (a medir)

### Fase 2
- [ ] SBC Orange Pi 5 Plus (100 × 75mm)
- [ ] Display RPi 7" Official (194 × 110mm)

### Fase 3
- [ ] SBC Orange Pi 6 Plus (TBD)
- [ ] SBC Raspberry Pi CM4 + IO Board
- [ ] Display 5" HDMI
- [ ] Display 10" IPS

## Stack de Software (referência)

O hardware foi projetado para rodar o OpenRig software stack:
- **OS**: Ubuntu/Debian ARM64
- **Runtime**: OpenRig (Rust + Slint GUI)
- **Audio**: JACK/PipeWire → LV2 plugins, NAM, IR loader
- **Controle**: USB HID do Controller Module → mapeado via OSC/MIDI
