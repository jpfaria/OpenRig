# OpenRig — 3D Modeling Instructions

> Instruções para o Claude Code sobre como modelar peças 3D para o OpenRig.
> Coloque este arquivo em `hardware/MODELING.md` no projeto.

## Setup

```bash
# Instalar OpenSCAD (Ubuntu/Debian no Claude Code)
apt-get update -qq && apt-get install -y -qq openscad xvfb

# Compilar SCAD para STL
xvfb-run -a openscad -o output.stl input.scad

# Compilar com variável (ex: selecionar peça)
xvfb-run -a openscad -o piece.stl -D 'part="top"' model.scad

# Render PNG para preview
xvfb-run -a openscad -o preview.png --imgsize=1920,1080 --viewall --colorscheme=Tomorrow model.scad
```

## Convenções OpenSCAD

### Estrutura do arquivo
```openscad
// === [Nome da Peça] ===
// Descrição

// === Parâmetros do usuário (topo do arquivo) ===
length = 100;    // mm
width  = 60;     // mm
wall   = 3;      // mm

// === Parâmetros derivados ===
inner_length = length - 2 * wall;

// === Módulos ===
module minha_peca() { ... }

// === Render ===
part = "both";  // "top", "bottom", "both", "exploded"
if (part == "top") { ... }
```

### Regras de design para impressão FDM

| Parâmetro | Mínimo | Recomendado |
|-----------|--------|-------------|
| Parede | 0.8mm | 2-3mm |
| Feature | 0.4mm | 0.8mm |
| Clearance (encaixe) | 0.1mm | 0.2-0.3mm |
| Overhang sem suporte | — | ≤45° |
| Bridge span | — | ≤10mm |
| Furo M2.5 clearance | 2.7mm | 2.7mm |
| Furo M3 clearance | 3.2mm | 3.2mm |
| Furo M3 tap (rosca no plástico) | 2.5mm | 2.5mm |
| Furo M4 clearance | 4.3mm | 4.3mm |
| Heat-set insert M3 | 4.0mm | 4.0mm |

### Mesa do João: 300 × 300mm
Todas as peças devem caber nessa mesa. Dividir peças grandes em metades.

### Padrão de furação dos bays
```openscad
// 8 furos M3 padrão — usar em todos os bays e carriers
module bay_holes(bay_w, bay_d, hole_d=3.2) {
    inset = 6;  // mm das bordas
    positions = [
        // 4 cantos
        [inset, inset],
        [bay_w - inset, inset],
        [inset, bay_d - inset],
        [bay_w - inset, bay_d - inset],
        // 4 intermediários
        [bay_w/2, inset],
        [bay_w/2, bay_d - inset],
        [inset, bay_d/2],
        [bay_w - inset, bay_d/2],
    ];
    for (pos = positions)
        translate([pos[0], pos[1], 0])
            cylinder(h=100, d=hole_d, center=true, $fn=16);
}
```

### Dovetail rail (encaixe entre módulos)
```openscad
// Perfil dovetail — trapézio 60°
module dovetail_male(length) {
    linear_extrude(height=length)
        polygon([
            [-6, 0], [6, 0],    // base 12mm
            [4, 8], [-4, 8]     // topo 8mm, altura 8mm
        ]);
}

module dovetail_female(length, clearance=0.3) {
    linear_extrude(height=length)
        polygon([
            [-6-clearance, -0.1], [6+clearance, -0.1],
            [4+clearance, 8+clearance], [-4-clearance, 8+clearance]
        ]);
}
```

### Reforço de footswitch
```openscad
// Pilar + anel para zona de footswitch
module fs_reinforcement(floor_z, panel_z) {
    // Pilar sólido do chão ao painel
    cylinder(h=panel_z - floor_z, d=16, $fn=24);
    // Anel reforçado na parte de baixo do painel
    translate([0, 0, panel_z - floor_z - 5])
        difference() {
            cylinder(h=5, d=22, $fn=24);
            cylinder(h=6, d=12.5 + 0.5, $fn=24);  // furo do FS
        }
}
```

## Arquitetura Modular (referência rápida)

```
Display (empilha em cima)     ← top+bottom, carrier para tela
   ↕ parafusos M3
OPi 5B (base do stack)        ← top+bottom, carrier para SBC
   ↔ dovetail lateral + M4
Audio / Blank plate           ← carrier com brackets ou vazia
   ↕ dovetail inferior + M4 + USB-C
Controller (footswitches)     ← top+bottom com reforço
```

## Hardware — Dimensões de Referência

### Orange Pi 5B
- PCB: 90 × 64mm
- Montagem: 4× M2.5, ~82 × 56mm spacing
- Standoff: 8mm altura

### Display Waveshare 7"
- PCB: 166.50 × 120.03mm
- Ativa: 157.45 × 89.90mm
- Montagem: 4× M2.5, 156.63 × 115.04mm spacing

### Scarlett 2i2
- Corpo: 175 × 99 × 47mm
- Sem furos — fixar com brackets laterais

### Controller
- 20× footswitch 12mm (furo 12.5mm)
- 10× pot 16mm (furo 7.5mm)
- Espaçamento: 46-52mm
- REFORÇAR zona dos footswitches

## Entregáveis por peça

Para cada peça modelada, gerar:
1. `.scad` — fonte parametrizado
2. `.stl` — compilado, pronto para slicer
3. `.png` — render para documentação
4. Atualizar `hardware/specs/` com dimensões finais
