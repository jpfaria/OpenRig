# OpenRig Hardware — Contexto de Design (Handoff)

> Este documento captura todas as decisões de design tomadas na sessão de projeto
> do hardware do OpenRig. Use como contexto ao continuar o trabalho no Claude Code.
> 
> Data: 2026-04-15
> Conversa: Claude.ai → Claude Code handoff

## Resumo Executivo

Projetamos uma **arquitetura modular de hardware** para o OpenRig — um processador de
áudio open-source para guitarra. O sistema usa peças impressas em 3D que empilham e
encaixam como LEGO, permitindo trocar componentes (SBC, display, interface de áudio)
sem reimprimir o chassis.

## Decisões de Design

### 1. Arquitetura Modular por Empilhamento

O sistema NÃO é um case monolítico. São **peças independentes que empilham e encaixam**:

```
        ┌─────────────────┐
        │   Display 7"    │  ← Empilha em cima do OPi (parafusos M3)
        ├─────────────────┤
        │   Orange Pi 5B  │  ← Base do "brain stack"
        └────────┬────────┘
                 │ encaixe lateral (dovetail + M4)
    ┌────────────┴────────────┐
    │  Audio Interface        │  ← OU blank plate (interface externa)
    │  (Scarlett 2i2 / Teyun) │
    └─────────────────────────┘
                 │ encaixe inferior (dovetail + USB-C)
    ┌────────────┴────────────────────────────────┐
    │  Controller Module                           │
    │  20 footswitches (2×10) + 10 potenciômetros  │
    └──────────────────────────────────────────────┘
```

### 2. Carrier Plates (Adaptadores Intercambiáveis)

Cada módulo aceita **carrier plates** — placas adaptadoras finas (~5mm) com:
- Furos M3 padrão no perímetro (interface com o módulo)
- Standoffs específicos do hardware (posição de montagem do SBC/display)
- I/O shield recortado (portas do modelo específico)

Trocar de hardware = reimprimir só a carrier plate (~30min de impressão).

### 3. Configurações Possíveis

| Config | Módulos | Uso |
|--------|---------|-----|
| Mini desktop | Display + OPi | Rack, mesa, estúdio |
| Integrado | Display + OPi + Scarlett | Unidade completa sem pedaleira |
| Full pedalboard | Display + OPi + Audio + Controller | Setup completo no chão |
| Controller standalone | Só o Controller | Controlador MIDI/HID genérico |

### 4. Mesa de Impressão

**Todas as peças devem caber em 300 × 300mm** (mesa do João).
Cada módulo imprime em top + bottom (acesso interno para montar componentes).

## Hardware Selecionado

### SBC: Orange Pi 5B
- PCB: 90 × 64mm
- SoC: Rockchip RK3588S
- RAM: até 16GB LPDDR4
- WiFi 6 + BT 5.3 integrado
- eMMC onboard
- Furos de montagem: 4× M2.5

### Display: Waveshare 7" (medido pelo João)
- PCB (OD): **166.50 × 120.03mm**
- Área ativa: **157.45 × 89.90mm**
- Furos de montagem: **156.63 × 115.04mm** (4× M2.5)
- Interface: HDMI + USB touch

### Interface de Áudio: Focusrite Scarlett 2i2
- Dimensões: 175 × 99 × 47mm
- Entradas: 2× Combo XLR/TRS
- Saídas: 2× TRS 1/4"
- Conexão: USB-C
- SEM furos de montagem (fixar com brackets laterais)

### Interface de Áudio: Teyun Q-26
- Dimensões: **A MEDIR** (João tem o equipamento)
- Status: Aguardando medições

### Controller
- 20 footswitches: momentary 12mm, furo ∅12.5mm
- 10 potenciômetros: 16mm, furo ∅7.5mm
- Espaçamento: 46-52mm center-to-center
- MCU: RP2040 ou STM32 (USB HID) ou GPIO direto do OPi
- Zona dos footswitches REFORÇADA (pilares + ribs + anéis)

## Carriers Planejados

### Fase 1 (agora)
- [x] Display Waveshare 7" — dimensões confirmadas
- [x] SBC Orange Pi 5B — dimensões confirmadas
- [ ] Audio Scarlett 2i2 — dimensões confirmadas, carrier a modelar
- [ ] Audio Teyun Q-26 — dimensões a medir
- [ ] Blank plate (sem interface) — a modelar

### Fase 2
- [ ] SBC Orange Pi 5 Plus (100 × 75mm, RK3588 full)
- [ ] Display RPi 7" Official (194 × 110mm, DSI)
- [ ] SBC Orange Pi 6 Plus (TBD)

### Fase 3
- [ ] SBC RPi CM4 + IO Board (compatível Elk Audio OS)
- [ ] Display 5" / 10"

## Padrões de Encaixe

### Empilhamento vertical (Display ↔ OPi)
- Parafusos M3 × 8mm (countersunk no topo)
- 8 furos no perímetro — padrão fixo
- Lip de alinhamento de 2mm

### Encaixe lateral (Brain stack ↔ Audio)
- Dovetail rail (trapézio 60°, 12mm topo, 8mm base, 8mm profundidade)
- 4× parafusos M4 para travar
- 2× dowel pins ∅6mm para alinhamento
- Clearance: 0.3mm por lado

### Encaixe inferior (Brain+Audio ↔ Controller)
- Dovetail rail ao longo do comprimento total
- 4× M4 + 2× dowel ∅6mm
- USB-C interno (15cm) para dados
- Slot de passagem de cabo 15×5mm

## O Que Já Foi Feito

### Documentação (neste zip)
- `hardware/README.md` — Overview do sistema
- `hardware/ARCHITECTURE.md` — Arquitetura detalhada, fluxo de sinal
- `hardware/specs/standard-interfaces.md` — Padrões de bay, dovetail, parafusos
- `hardware/specs/brain-frame.md` — Chassis (versão anterior, monolítico)
- `hardware/specs/controller-module.md` — Controller com reforço
- `hardware/specs/carrier-sbc.md` — Carriers de SBC com templates
- `hardware/specs/carrier-display.md` — Carriers de display
- `hardware/specs/carrier-audio.md` — Carriers de interface de áudio

### SCAD (protótipo v6 — monolítico, será refatorado)
- `hardware/scad/pedalboard-v6.scad` — Modelo OpenSCAD anterior
  - Este arquivo é o protótipo monolítico (uma peça só)
  - Precisa ser refatorado para a nova arquitetura modular
  - Tem o reforço dos footswitches (pilares + ribs) — reutilizar
  - Tem as dimensões do display Waveshare — reutilizar

### Skill de 3D Printing
- Criamos e instalamos uma skill de modelagem 3D para o Claude
- Usa OpenSCAD para gerar STLs
- Já testado: OpenSCAD está disponível via `apt-get install openscad`
- Renderiza com: `xvfb-run -a openscad -o output.stl input.scad`

### HTML 3D Viewer
- Viewer standalone em HTML + Three.js (abre no Chrome)
- Sem dependência de JSX/React
- `openrig_3d_viewer.html` — última versão com display correto

## O Que Falta Fazer

### Prioridade 1: Modelar a nova arquitetura modular
1. **Módulo OPi 5B** (base do stack) — top + bottom, carrier plate
2. **Módulo Display** (empilha em cima) — top + bottom, carrier plate
3. **Encaixe vertical** entre Display e OPi
4. **Módulo Audio Scarlett** — carrier com brackets laterais
5. **Blank plate** — placeholder sem interface
6. **Encaixe lateral** (dovetail) entre Brain stack e Audio
7. **Controller Module** — refatorar do v6 com reforço
8. **Encaixe inferior** (dovetail) entre Brain+Audio e Controller

### Prioridade 2: Biblioteca OpenSCAD
- `lib/dovetail.scad` — módulo de encaixe reutilizável
- `lib/carrier-base.scad` — base para carrier plates
- `lib/bay.scad` — bay padrão com furos M3
- `lib/reinforcement.scad` — pilares e ribs para footswitches

### Prioridade 3: Documentação adicional
- Atualizar specs para a nova arquitetura de empilhamento
- BOM (Bill of Materials) com links de compra
- Guia de montagem passo a passo
- Medir Teyun Q-26

## Contexto do Projeto OpenRig (Software)

- Repo: `github.com/jpfaria/OpenRig` (privado)
- Linguagem: Rust
- GUI: Slint
- Audio: LV2 plugin hosting, NAM, IR loading
- Path local: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig`
- O João usa Claude Code com `bypassPermissions` mode
- Desktop funcional com plugin graph
- Pesquisou hardware: Orange Pi 5B, CM4 + Elk Audio OS, SHARC DSP
