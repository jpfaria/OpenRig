# OpenRig Hardware

Arquitetura modular de hardware para o OpenRig — um sistema open-source de processamento de áudio para guitarra.

## Filosofia

O hardware do OpenRig segue o princípio **LEGO**: peças padronizadas que encaixam, parafusam, e podem ser trocadas sem reimprimir o chassis inteiro. Trocar de Orange Pi 5B para 6 Plus? Reimprima só a carrier plate (~30min). Trocar a Scarlett por outra interface? Mesma coisa.

## Arquitetura

```
┌─────────────────────────────────────────────────────┐
│                  BRAIN FRAME                         │
│  (chassis universal — nunca muda)                    │
│                                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐   │
│  │ Display  │  │   SBC    │  │  Audio Interface  │   │
│  │   Bay    │  │   Bay    │  │       Bay         │   │
│  │ 180×130  │  │ 120×100  │  │    190×110        │   │
│  │          │  │          │  │                    │   │
│  └──────────┘  └──────────┘  └──────────────────┘   │
│       ↕              ↕              ↕                │
│   carrier        carrier        carrier              │
│    plate          plate          plate                │
└─────────────────────────────────────────────────────┘
          │ dovetail rail + USB-C │
┌─────────────────────────────────────────────────────┐
│              CONTROLLER MODULE                       │
│  20 footswitches (2 rows × 10)                      │
│  10 potentiometers (1 row × 10)                     │
│  (encaixa ou funciona separado)                     │
└─────────────────────────────────────────────────────┘
```

## Módulos

### Brain Frame
O chassis principal. Contém 3 bays padronizados com furos M3. Cada bay aceita carrier plates intercambiáveis.

→ [Especificação completa](specs/brain-frame.md)

### Controller Module
Painel de controle com footswitches e potenciômetros. Conecta no Brain Frame via dovetail rail (encaixe mecânico) + USB-C (dados).

→ [Especificação completa](specs/controller-module.md)

### Carrier Plates
Placas adaptadoras finas (~5mm) que fazem a ponte entre o bay padronizado e o hardware específico.

| Tipo | Hardware Suportado | Spec |
|------|-------------------|------|
| SBC | Orange Pi 5B, 5 Plus, 6 Plus, RPi CM4 | [carrier-sbc.md](specs/carrier-sbc.md) |
| Display | Waveshare 7" (166×120mm), RPi 7" Official, 5" HDMI | [carrier-display.md](specs/carrier-display.md) |
| Audio | Focusrite Scarlett 2i2, Teyun Q-26 | [carrier-audio.md](specs/carrier-audio.md) |

## Modos de Uso

### Modo Mini (só o Brain)
Brain Frame com display + SBC + interface de áudio. Funciona como unidade desktop compacta. Conecta direto no amp via interface de áudio.

### Modo Full (Brain + Controller)
Brain Frame encaixado no Controller Module. Pedalboard completo estilo Helix/Headrush.

### Modo Controller Standalone
Controller Module sozinho, conectado via USB-C a um computador ou outro Brain. Funciona como controlador MIDI/OSC genérico.

## Impressão 3D

- **Mesa mínima**: 300 × 300mm
- **Material recomendado**: PETG (resistência + leve flexibilidade)
- **Infill**: 40% para estrutural, 20% para carriers
- **Espessura de parede**: 3mm (frame), 2mm (carriers)
- **Cada peça imprime em top + bottom** para acesso interno

## Estrutura de Arquivos

```
hardware/
├── README.md                    # Este arquivo
├── ARCHITECTURE.md              # Visão geral da arquitetura
├── specs/
│   ├── brain-frame.md           # Especificação do chassis
│   ├── controller-module.md     # Especificação do controller
│   ├── carrier-sbc.md           # Carriers de SBC
│   ├── carrier-display.md       # Carriers de display
│   ├── carrier-audio.md         # Carriers de interface de áudio
│   └── standard-interfaces.md   # Padrões de bay e encaixe
├── scad/
│   ├── brain-frame.scad         # OpenSCAD do chassis
│   ├── controller-module.scad   # OpenSCAD do controller
│   └── lib/                     # Módulos compartilhados
│       ├── bay.scad             # Módulo de bay padrão
│       ├── dovetail.scad        # Módulo de encaixe dovetail
│       └── carrier-base.scad    # Base para carrier plates
├── carriers/
│   ├── sbc-opi5b.scad           # Carrier Orange Pi 5B
│   ├── sbc-opi5plus.scad        # Carrier Orange Pi 5 Plus
│   ├── display-waveshare7.scad  # Carrier Waveshare 7"
│   ├── audio-scarlett2i2.scad   # Carrier Scarlett 2i2
│   └── audio-teyun-q26.scad    # Carrier Teyun Q-26
└── stl/                         # STLs prontos para impressão
    └── (gerados via Makefile)
```

## Contribuindo

Para adicionar suporte a um novo hardware:

1. Meça o PCB/dispositivo (comprimento, largura, altura, posição dos furos de montagem, posição das portas)
2. Copie a carrier mais parecida em `carriers/`
3. Ajuste os parâmetros no topo do arquivo
4. Compile com `openscad -o stl/carrier-nome.stl carriers/nome.scad`
5. Teste o encaixe imprimindo a carrier
6. Documente em `specs/carrier-*.md`
7. Abra um PR

## Licença

Hardware design: [CERN-OHL-S-2.0](https://ohwr.org/cern_ohl_s_v2.txt) (Open Hardware License)
