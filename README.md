# Pedaleira Workspace

Workspace profissional da pedaleira, preparado para múltiplas interfaces:

- console
- gRPC
- GUI
- plugin VST3

## Princípios

- domínio independente de YAML, banco, gRPC e GUI
- estado lógico separado do runtime
- engine como projeção executável do estado
- persistência por portas/repositórios
- adapters independentes

## Crates

- `pedal-domain`: entidades de domínio
- `pedal-setup`: estrutura da pedalboard
- `pedal-preset`: presets
- `pedal-state`: estado lógico, comandos e eventos
- `pedal-ports`: contratos de persistência e integração
- `pedal-application`: casos de uso
- `pedal-engine`: runtime / engine
- `pedal-nam`: integração NAM
- `pedal-ir`: IR / WAV
- `pedal-infra-*`: infraestrutura concreta
- `pedal-adapter-*`: interfaces de entrada/saída
