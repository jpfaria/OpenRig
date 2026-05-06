# OpenRig — Claude Code

Pedalboard/rig virtual para guitarra em Rust + Slint. Cross-platform: macOS, Windows, Linux.

## Invariantes que NUNCA podem piorar

OpenRig é áudio em tempo real. **Qualidade sonora e latência são os valores centrais.** Toda mudança que toca audio thread, DSP, roteamento, I/O ou cadeia de blocos precisa provar que não regride NADA abaixo:

1. **Latência round-trip.**
2. **Qualidade de áudio** (ruído, aliasing, THD, resposta em frequência).
3. **Estabilidade do stream** — zero xruns, dropouts, cliques, pops.
4. **Isolation entre streams** — cada `InputBlock` é um runtime paralelo TOTALMENTE isolado. Sem buffer/lock/route/tap compartilhado. "Chain" no YAML é só agrupamento lógico. Mixing entre streams acontece no backend (cpal/JACK), nunca no nosso código.
5. **Stream é SEMPRE estéreo internamente.** Mono input → broadcast `Stereo([s,s])`. DualMono → `Stereo([L,R])` independentes. Stereo → direto. Output só vira mono se `OutputBlock.mode == mono` (via `apply_mixdown`). NUNCA forçar bus mono no segment quando output é estéreo. NUNCA auto-pan.
6. **Jitter do callback** estável.
7. **Custo de CPU no audio thread** — regressão vira xrun.
8. **Zero alocação, lock, syscall ou I/O no audio thread.** Sem exceção.
9. **Determinismo numérico** — golden samples passam dentro da tolerância.
10. **Volume por stream IMUTÁVEL** sem pedido explícito do usuário. Se `volume_invariants_tests.rs` quebra, **a fonte está errada, não o teste**. Pinned em `crates/engine/src/volume_invariants_tests.rs`.

### Red flags — PARAR e reportar

- Novo xrun, dropout, clique
- Latência sobe > 1ms sem justificativa
- Golden tests falham
- `Mutex`/`RwLock`/log/print/file I/O no processamento
- "Em macOS/Windows/Linux o som mudou" → regressão, não compatibilidade
- "Volume ficou diferente" → regressão. NUNCA ajuste o teste de invariância
- Buffer/runtime state/tap/route compartilhado entre 2+ streams → viola isolation

### Hierarquia de trade-offs

1. Som + estabilidade + isolation (empate)
2. Latência
3. CPU no audio thread
4. Cross-platform
5. Ergonomia de código
6. Funcionalidade nova

Feature nova **não justifica** regressão. Trade-off → discutir antes.

### Cross-platform

- NUNCA hardcodar paths. macOS `~/Library/Application Support/OpenRig/`, Windows `%APPDATA%\OpenRig\`, Linux `~/.local/share/openrig/`.
- Fix de Linux/Orange Pi/JACK fica atrás de `cfg` guards. NUNCA mudar comportamento cross-platform pra resolver UM SO.

## Regras gerais de código

- **Zero warnings** (`cargo build` limpo).
- **Zero acoplamento** — blocos não referenciam modelos/brands específicos.
- **Single source of truth** — constantes uma vez.
- **Separação de concerns** — business logic sem config visual/UI.
- Documentação é parte da tarefa: mudou modelo/block/parâmetro/tela/comportamento de áudio → atualizar `docs/` no mesmo commit.

## Referências (ler quando precisar)

| Doc | Quando |
|---|---|
| `docs/development/gitflow.md` | Issues, branches, commits, fechamento, workspace `.solvers/`, comentários |
| `docs/development/file-organization.md` | Onde mora cada coisa, caps de LOC, LV2 audio_mode |
| `docs/hardware/orange-pi-deploy.md` | Alterar SO da placa via `platform/orange-pi/` |
| `docs/blocks-catalog.md` | Tipos de bloco, modelos, parâmetros, backends |
| `docs/screens.md` | Telas (Launcher, Chains, Tuner, Spectrum, Block Editor) |
| `docs/cli.md` | Args e env vars do `openrig` |
| `docs/scripts.md` | Build/deploy, fluxo .deb→Orange Pi |
| `docs/audio-config.md` | I/O como blocos, JACK lifecycle |
| `docs/architecture.md` | Crates, registry, assets, BlockEditorPanel |
| `docs/testing.md` | Cobertura, convenções, comandos |
| `CONTRIBUTING.md` | Detalhes de regras de código |
