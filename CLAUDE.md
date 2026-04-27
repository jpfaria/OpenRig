# OpenRig — Contexto do Projeto para Claude Code

## OBRIGATORIO — Skills

**Antes de qualquer ação:** invocar `superpowers:using-superpowers`. Vale para todos os agentes (locais e GitHub Actions).

**Ao tocar em código:**
- Rust → `openrig-code-quality` + `rust-best-practices`
- Slint (`.slint`) → `slint-best-practices`

**Por situação (invocar antes de agir):**

| Situação | Skill |
|---|---|
| Adicionando feature ou comportamento novo | `superpowers:brainstorming` |
| Implementando feature ou bugfix | `superpowers:test-driven-development` |
| Debugando bug ou falha de teste | `superpowers:systematic-debugging` |
| Executando plano já escrito | `superpowers:executing-plans` |
| Trabalho completo, prestes a declarar "done" | `superpowers:verification-before-completion` |
| Recebendo feedback de code review | `superpowers:receiving-code-review` |
| Finalizando branch, prestes a abrir PR | `superpowers:finishing-a-development-branch` |
| Criando nova skill | `superpowers:writing-skills` |
| 2+ tarefas independentes em paralelo | `superpowers:dispatching-parallel-agents` |

Nenhuma é opcional.

---

## OBRIGATORIO — Prioridades de Produto (Non-Regression)

OpenRig é um processador de áudio em tempo real. **Qualidade sonora e latência são os valores centrais.** Toda mudança deve provar que não degrada nenhuma propriedade abaixo antes de mergear.

### Invariantes que NUNCA podem piorar

1. **Latência round-trip**
2. **Qualidade de áudio** (ruído, aliasing, THD, resposta em frequência)
3. **Estabilidade do stream** — zero xruns, dropouts, cliques, glitches, pops
4. **Jitter do callback** — tempo de processamento estável
5. **Custo de CPU no audio thread** — regressão de CPU vira xrun
6. **Zero alocação, lock, syscall ou I/O no audio thread** — sem exceção
7. **Determinismo numérico** — golden samples passam dentro da tolerância

### Checklist obrigatório antes do PR/merge

Se a mudança toca audio thread, DSP, roteamento, I/O ou cadeia de blocos, responder no PR/issue:

- [ ] Afeta o audio thread? Medi CPU/callback antes e depois? Escutei ≥60s sem glitch?
- [ ] Afeta latência? Qual o delta em ms? Justificado?
- [ ] Afeta o som de algum bloco? Golden tests passando? Fiz A/B auditivo?
- [ ] Introduz alocação, lock, syscall ou lazy init no hot path? Se sim, reverter.

### Red flags — PARAR e reportar

- Novo xrun, dropout ou clique audível
- Latência sobe > 1ms sem justificativa
- Golden sample tests falham
- Pico de callback acima do buffer period
- Necessidade de `Mutex`/`RwLock`/`Arc::clone`/log/print/file I/O no processamento
- "Em macOS/Windows/Linux o som mudou" → é regressão, não compatibilidade

### Hierarquia de trade-offs

1. Qualidade do som **e** estabilidade do stream (empate no topo)
2. Latência
3. Custo de CPU no audio thread
4. Compatibilidade cross-platform
5. Ergonomia de código
6. Funcionalidade nova

Feature nova **não justifica** regressão. Trade-off nesses eixos → **discutir com o usuário antes**.

---

## O que é o OpenRig

Pedalboard/rig virtual para guitarra em Rust. Processa áudio em cadeia (chain) com blocos (blocks) de efeitos e amplificadores. UI em Slint. Distribuição cross-platform: macOS, Windows, Linux.

---

## Fluxo de Desenvolvimento — Gitflow (OBRIGATORIO)

```
Issue → Branch (from develop) → Commits → PR → Review/Merge
```

| Branch | Propósito | Merge into |
|--------|-----------|------------|
| `main` | Releases | — |
| `develop` | Integração para próxima release | `main` |
| `feature/*` | Novas funcionalidades | `develop` |
| `bugfix/*` | Correções de bugs | `develop` |
| `hotfix/*` | Urgências em produção | `main` + `develop` |
| `release/*` | Preparação de release | `main` + `develop` |

### Regras

1. **Issue primeiro** — criar no GitHub antes de qualquer código. Sempre `gh issue list --search` antes de criar (evitar duplicatas).
2. **UMA branch por issue, nome `feature/issue-{N}` ou `bugfix/issue-{N}`** — NUNCA sufixo descritivo (`-fix`, `-v2`, `-parameter-layout` etc). Antes de criar, `git fetch && git branch -a | grep "issue-{N}"`. Se existe, usar; se precisa recomeçar, resetar a existente.
3. **Sempre a partir de develop atualizado** — `git checkout develop && git pull` antes de criar branch.
4. **Mergear develop antes de qualquer trabalho** — `git merge -X theirs origin/develop` (develop tem prioridade em conflitos).
5. **Commits em inglês**, sem `Co-Authored-By`, foco no "why".
6. **NUNCA `Closes #N` ou `Fixes #N` em commits** — GitHub auto-fecha issues.
7. **Merge policy**: bugfix/hotfix mergeia imediato; feature aguarda review. Nunca mergear feature→develop sem o usuário pedir.
8. **NUNCA rebase** — sempre `git merge`, nunca `git pull --rebase`.
9. **NUNCA fechar issues** — só quando o usuário pedir. **Ao fechar, sempre atribuir ao milestone aberto vigente** (`gh api repos/<owner>/<repo>/milestones --jq '.[] | select(.state=="open") | .title'` para descobrir; `gh issue edit <N> --milestone "<title>"` para atribuir antes do `gh issue close <N>`). Issue fechada sem milestone não aparece nos relatórios de release.
10. **Push imediato após cada commit.**

### Workspace isolado (.solvers/)

**NUNCA editar código no workspace principal.** Cada agent trabalha numa cópia:

```
OpenRig/                  ← workspace principal (read-only para código)
  .solvers/issue-{N}/     ← cópia isolada por issue
  .solvers/doc/           ← cópia para documentação (branch main)
```

```bash
rsync -a --exclude='target' --exclude='.solvers' . .solvers/issue-{N}/
cd .solvers/issue-{N} && git fetch origin
# branch existe? checkout. não existe? checkout develop && pull && checkout -b feature/issue-{N}
```

Leitura/exploração no workspace principal é OK. Após push, sugerir só `git checkout <branch> && git pull` ao usuário (ele trabalha no principal). Após merge+close de issue, `rm -rf .solvers/issue-{N}/`.

### Issues irmãs — co-evolução obrigatória

Quando duas issues tocam os mesmos arquivos, são **irmãs** e co-evoluem. Identificação: o **corpo** (não comentário) começa com bloco quote `> **Sibling issues (co-evoluem neste ciclo):** #<outra>`. Antes de QUALQUER nova implementação numa issue irmã, `git fetch && git merge origin/feature/issue-<irma> --no-edit && cargo build --workspace`. Sync a cada commit lógico, não só no início. Ao descobrir overlap, editar o body das DUAS issues com o bloco (navegação simétrica).

### Rastreabilidade — comentários obrigatórios na issue

A issue é o log de auditoria. Commits dizem o "o que"; decisões, conflitos, análises, respostas técnicas se perdem se não forem registradas.

**Comentar em:** plano antes de começar; cada push (hash + arquivos + resultado de build/teste); mudança de plano (antes de executar); cada problema com evidência mínima; cada análise técnica não-trivial; resposta técnica relevante dada ao usuário; merges (o que veio, conflitos, resoluções); validação em hardware; resumo final.

**Regra prática:** depois de todo `git push` ou análise técnica, o próximo comando é `gh issue comment <N>`. Em dúvida, comente — excesso tem custo zero, ausência custa o trabalho de reconstruir decisões. **Opções A/B/C ao usuário** vão na issue ANTES da pergunta; resposta dele também vira comentário.

### Cross-platform e distribuição

OpenRig roda em **macOS, Windows, Linux**:

- NUNCA hardcodar paths
- Paths de assets via config central (LV2, NAM, IR captures)
- Por plataforma: macOS `~/Library/Application Support/OpenRig/`, Windows `%APPDATA%\OpenRig\`, Linux `~/.local/share/openrig/`
- Antes de qualquer decisão: "isso funciona se o usuário instalar no Windows?"
- **Isolamento absoluto**: fix de Linux/Orange Pi/JACK fica atrás de `cfg` guards. NUNCA mudar comportamento cross-platform pra resolver UM SO.

### Documentação é parte da tarefa

CLAUDE.md sempre reflete o estado atual. Mudança em modelo, block type, parâmetro, tela ou comportamento de áudio → atualizar CLAUDE.md no mesmo commit. Feature não documentada é dívida técnica.

### Alterações no SO da placa (Orange Pi)

Toda alteração no SO da placa TEM que ter equivalente em `platform/orange-pi/` antes de encerrar — patch que só vive na placa evapora no próximo flash.

| Alteração na placa | Arquivo no projeto |
|---|---|
| Kernel cmdline (`armbianEnv.txt extraargs=`) | `platform/orange-pi/customize-image.sh` (`KERNEL_ARGS`) |
| Systemd unit | `platform/orange-pi/rootfs/etc/systemd/system/` |
| Systemd drop-in | `platform/orange-pi/rootfs/etc/systemd/system/<unit>.d/` |
| `/etc/` config (sysctl, security, udev) | `platform/orange-pi/rootfs/etc/` |
| Binário em `/usr/local/bin/` | `platform/orange-pi/rootfs/usr/local/bin/` |
| Device Tree overlay | `platform/orange-pi/dtbo/` |
| Runtime (chown, groupadd, setcap, mkdir) | bloco em `customize-image.sh` |

**Ordem:** alterar no projeto → commit/push → aplicar na placa → validar. Patch experimental na placa é OK, mas voltar ao projeto antes de declarar resolvido. Pergunta de validação: "se o usuário flashar imagem nova agora, o fix continua lá?".

### Regras gerais de código

- **Zero warnings** — `cargo build` limpo
- **Zero acoplamento** — blocos não referenciam modelos, brands ou effect types específicos
- **Single source of truth** — constantes uma vez, nunca duplicadas
- **Separação de concerns** — business logic não tem config visual/UI

Ver `CONTRIBUTING.md` para detalhes.

---

## O Produto (visão do usuário)

Pedalboard virtual: usuário monta cadeia de efeitos visualmente, ajusta parâmetros em tempo real, toca com áudio profissional.

### Telas principais

- **Launcher** — criar/abrir projetos
- **Project Setup** — pede o nome ao criar novo projeto
- **Chains** — visualização da cadeia de blocos, drag/reorder
- **Block Editor** — edita parâmetros de um bloco
- **Compact Chain View** — power switches e troca rápida de modelo
- **Settings** — devices de áudio, sample rate, buffer size
- **Chain Editor** — nome, instrumento, I/O blocks
- **Tuner** — janela com 1 tuner por canal ativo (`feature-dsp/pitch_yin.rs` + `engine/input_tap.rs` + `adapter-gui/tuner_session.rs`)
- **Spectrum** — analisador 63-band 1/6 octava por canal de Output (`feature-dsp/spectrum_fft.rs` + `engine/output_tap.rs` + `adapter-gui/spectrum_session.rs`)

Janela principal inicia em 1100×620px lógicos.

### CLI / env vars (adapter-gui)

| Argumento / Variável | Efeito |
|---|---|
| `openrig /path/project.yaml` (posicional) | Abre direto, pula launcher |
| `OPENRIG_PROJECT_PATH=...` | Igual ao posicional (env tem menor prioridade) |
| `--auto-save` ou `OPENRIG_AUTO_SAVE=1` | Salva a cada alteração, esconde botão |

Parsing em `adapter-gui/src/{main,lib}.rs`. Auto-save em `sync_project_dirty()`.

### Tipos de bloco

| Tipo | O que faz | Total | Modelos (resumo) |
|------|-----------|-------|-----------------|
| **Preamp** | Pré-amp, gain, EQ | 26 | American Clean, Brit Crunch, Modern High Gain (native); JCM 800 2203, Thunder 50, '57 Champ/Deluxe, Frontman 15G, PA100, Bantamp Meteor, AVT50H, YJM100, Mark III, Micro Terror, Shaman, Classic 30, MIG-100, VX Kraken, MIG-50, 22 Caliber, Blues Baby 22, Fly, Multitone 50, L2, Lunchbox Jr (NAM) |
| **Amp** | Preamp + power amp + cab | 29 | Blackface Clean, Tweed Breakup, Chime (native); Bogner Ecstasy/Shiva, Dumble ODS, EVH 5150, Friedman BE100, Marshall JCM800/JVM/JMP-1, Mesa Mark V/Rectifier, Peavey 5150, Ampeg SVT, Fender Bassman/Deluxe Reverb/Super Reverb, Roland JC-120B, Vox AC30/Fawn (NAM); GxBlueAmp, GxSupersonic, MDA Combo (LV2) |
| **Cab** | Caixa/falante | 17 | American 2x12, Brit 4x12, Vintage 1x12 (native); Celestion Cream, Fender Deluxe, Greenback, G12T-75, Marshall 4x12 V30, Mesa OS/Standard 4x12, Roland JC-120, Vox AC30 Blue, Vox AC50 (IR); GxUltraCab (LV2) |
| **Gain** | Overdrive, distortion, fuzz, boost | 91 | TS9 (native); Boss DS-1/HM-2/FZ-1W/MT-2/BD-2, Klon, RAT/RAT2, OCD, OD808, TS808, Darkglass Alpha Omega/B7K, JHS Bonsai, Bluesbreaker, Vemuram Jan Ray + 34 outros (NAM); Guitarix ×40, CAPS, OJD, Wolf Shaper, MDA (LV2) |
| **Delay** | Eco | 14 | Analog Warm, Digital Clean, Slapback, Reverse, Modulated, Tape Vintage (native); MDA DubDelay, TAP Doubler/Echo/Reflector, Bollie, Avocado, Floaty, Modulay (LV2) |
| **Reverb** | Ambiência | 19 | Hall, Plate Foundation, Room, Spring (native); Dragonfly Hall/Room/Plate/Early, CAPS Plate/X2/Scape, TAP Reflector/Reverberator, MDA Ambience, MVerb, B Reverb, Roomy, Shiroverb, Floaty (LV2) |
| **Modulation** | Chorus, flanger, tremolo, vibrato | 16 | Classic/Stereo/Ensemble Chorus, Sine Tremolo, Vibrato (native); TAP Chorus/Flanger/Tremolo/Rotary, MDA Leslie/RingMod/ThruZero, FOMP, CAPS Phaser II, Harmless, Larynx (LV2) |
| **Dynamics** | Compressor e gate | 9 | Studio Clean Compressor, Noise Gate, Brick Wall Limiter (native); TAP DeEsser/Dynamics/Limiter, ZamComp, ZamGate, ZaMultiComp (LV2) |
| **Filter** | EQ, moldagem tonal | 13 | Three Band EQ, Guitar EQ, 8-Band Parametric EQ (native); TAP Equalizer/BW, ZamEQ2, ZamGEQ31, CAPS AutoFilter, FOMP Auto-Wah, MOD HPF/LPF, Filta, Mud (LV2) |
| **Wah** | Wah-wah | 2 | Cry Classic (native); GxQuack (LV2) |
| **Body** | Ressonância de corpo acústico | 114 | Martin (45), Taylor (30), Gibson (10), Yamaha (5), Guild (4), Takamine (4), Cort (4), Emerald (2), Rainsong (2), Lowden (2) + boutique (IR) |
| **Pitch** | Pitch shift e harmonização | 4 | Harmonizer, x42 Autotune, MDA Detune, MDA RePsycho (LV2) |
| **IR** / **NAM** | Loaders genéricos | 1+1 | generic_ir, generic_nam |
| **Input** / **Output** / **Insert** | I/O | — | standard, standard, external_loop |

**Total: 360+ modelos em 16 tipos (5 backends: Native 33, NAM 89, IR 127, LV2 105, VST3 6).**

`Utility` está vazio (Tuner e Spectrum viraram features de toolbar). `Full Rig` reservado para futuras capturas com cadeia completa.

### Parâmetros comuns

- **Preamp/Amp nativos**: input, gain, bass, middle, treble, presence, depth, sag, master, bright
- **NAM preamp**: volume (50–70%), gain (10–100%) em steps
- **Delay**: time_ms (1–2000), feedback (0–100%), mix (0–100%)
- **Reverb**: room_size, damping, mix (0–100%)
- **Compressor**: threshold, ratio, attack_ms, release_ms, makeup_gain, mix
- **Gate** (`gate_basic`): threshold (-96 a 0 dB), attack_ms (0.1–100), release_ms (1–500), **hold_ms** (0–2000, default 150 — evita cortar decay), **hysteresis_db** (0–20, default 6 — evita chattering)
- **EQ (Three Band / Guitar EQ)**: low, mid, high (0–100% → -24/+24 dB)
- **8-Band Parametric EQ** (`eq_eight_band_parametric`): por banda — `band{N}_enabled`, `band{N}_type` (peak/low_shelf/high_shelf/low_pass/high_pass/notch), `band{N}_freq` (20–20000 Hz), `band{N}_gain` (-24/+24 dB), `band{N}_q` (0.1–10). Freqs padrão: 62/125/250/500/1k/2k/4k/8kHz.
- **Gain pedals**: drive, tone, level
- **NAM gain pedals com grid**: knobs reais por modelo (`tone`, `sustain`, `drive`, `volume`, `gain`...) mapeiam para captura `.nam` mais próxima na grid. Sufixo `_feather`/`_lite`/`_nano` vira enum `size`. Pedais com nomes nominais (`chainsaw`, `medium`) ou `preset_N` mantêm enum dropdown. Codegen: `tools/gen_pedal_models.py`.
- **Volume**: volume (0–100%), mute
- **Vibrato**: rate_hz (0.1–8), depth (0–100%), 100% wet
- **Autotune Chromatic**: speed (0–100ms), mix, detune (±50 cents), sensitivity
- **Autotune Scale**: + key (C–B), scale (Major, Minor, Pent Maj/Min, Harmonic Minor, Melodic Minor, Blues, Dorian)

### Backends de áudio

- **Native** — DSP em Rust, mais rápido
- **NAM** — Neural Amp Modeler
- **IR** — Impulse Response (cabs, corpos)
- **LV2** — Plugins externos open-source

### Instrumentos

`electric_guitar`, `acoustic_guitar`, `bass`, `voice`, `keys`, `drums`, `generic`. Constantes em `crates/block-core/src/lib.rs` (`INST_*`, `ALL_INSTRUMENTS`, `GUITAR_BASS`, `GUITAR_ACOUSTIC_BASS`).

Cada `MODEL_DEFINITION` tem `supported_instruments: &[&str]`. UI filtra a lista de blocos disponíveis. Campo `instrument` salvo no YAML da chain, default `electric_guitar`, fixo após criação.

### Configuração de áudio — I/O como blocos

`Input`, `Output`, `Insert` são variantes de `AudioBlockKind` dentro de `chain.blocks`. Não existem listas separadas.

- `blocks[0]` = InputBlock (fixo, não removível)
- `blocks[N-1]` = OutputBlock (fixo, não removível)
- Inputs/Outputs/Inserts extras podem ser inseridos no meio
- Cada Input cria stream paralelo isolado; Output é tap não-destrutivo
- Insert divide a chain em segmentos; desabilitado = bypass (sinal passa direto)

Exemplo YAML mínimo:

```yaml
chains:
  - description: guitar 1
    instrument: electric_guitar
    blocks:
      - { type: input,  model: standard, enabled: true, entries: [{ name: In1, device_id: "...", mode: mono, channels: [0] }] }
      - { type: preamp, model: marshall_jcm_800_2203, enabled: true, params: { volume: 70, gain: 40 } }
      - { type: insert, model: external_loop, enabled: true, send: {...}, return_: {...} }
      - { type: delay,  model: digital_clean, enabled: true, params: { time_ms: 350, feedback: 40, mix: 30 } }
      - { type: output, model: standard, enabled: true, entries: [{ name: Out1, device_id: "...", mode: stereo, channels: [0,1] }] }
```

Sample rates: 44.1/48/88.2/96 kHz. Buffer sizes: 32/64/128/256/512/1024. Bit depths: 16/24/32. YAML antigo (`inputs:`/`outputs:` separados, `input_device_id`/`output_device_id` únicos) é migrado automaticamente.

### Per-machine device settings (gui-settings.yaml)

Sample rate, buffer size, bit depth são **per-machine**, não per-project. Ficam em:
- macOS: `~/Library/Application Support/OpenRig/gui-settings.yaml`
- Windows: `%APPDATA%\OpenRig\gui-settings.yaml`
- Linux: `~/.config/OpenRig/gui-settings.yaml`

`load_project_session()` popula `project.device_settings` em memória. YAML do projeto **não persiste** `device_settings` (`skip_serializing`), mas YAML antigo com o campo ainda deserializa.

### JACK lifecycle (Linux only)

Com feature `jack`, OpenRig controla o ciclo de vida do JACK. `ensure_jack_running()` em infra-cpal detecta a placa USB, lê SR/buffer do `device_settings`, configura mixer ALSA, lança `jackd -d alsa -d hw:$CARD -r $SR -p $BUF -n 3`, espera o socket aparecer em `/dev/shm/`. Timer de 2s no adapter-gui (`health_timer`) verifica `is_healthy()` e tenta reconectar quando JACK volta. Tudo atrás de `#[cfg(all(target_os = "linux", feature = "jack"))]`.

---

## Arquitetura — crates

- `block-core` — `BlockProcessor`, `AudioChannelLayout`, `ParameterSet`, constantes de instrumento
- `block-preamp` / `block-amp` — preamp / amp completo
- `adapter-gui` — UI Slint (`ui/`)
- `nam` — Neural Amp Modeler
- `asset-runtime` — `EmbeddedAsset`, `materialize()`

### Brand/type ficam no Rust, não no YAML

`PreampModelDefinition` (em `crates/block-preamp/src/registry.rs`) tem `id`, `display_name`, `brand`, `backend_kind`, `schema`, `validate`, `asset_summary`, `build`. Funções públicas: `preamp_display_name`, `preamp_brand`, `preamp_type_label` (`"native" | "NAM" | "IR"`).

`component.yaml` só tem caminhos de assets e posições SVG (`svg_cx`, `svg_cy`).

### Registry auto-gerado

`crates/block-preamp/build.rs` escaneia `src/*.rs` procurando `MODEL_DEFINITION` e gera `generated_registry.rs`. Novo modelo = criar `.rs` com `pub const MODEL_DEFINITION: PreampModelDefinition = ...`.

### Assets

```
assets/brands/{marshall,vox,native}/logo.svg   ← logos worldvectorlogo (Marshall: fill="currentColor"; Vox: #53ad99 + #d99346)
assets/amps/{brand}/{model}/controls.svg       ← painel completo (não criar panel.svg separado)
assets/amps/{brand}/{model}/component.yaml     ← caminhos de assets + svg_cx/cy
```

`controls.svg` usa o AC30 como template visual: viewBox 800×200, fundo escuro, círculos como âncoras de knob (`fill="#111" stroke="#505050"`). Controles editáveis têm `id="ctrl-xxx"`; não-editáveis usam `opacity="0.6"` sem id. Logo do brand NUNCA dentro da imagem do equipamento.

### BlockEditorPanel

Quando o bloco selecionado é `preamp`, o painel mostra `controls.svg` em vez de só sliders. Implementação em `crates/adapter-gui/ui/pages/project_chains.slint` (propriedades `is-preamp`, `selected-model-id`, ternary chain de `@image-url()` por compile-time). `amp` ainda não tem equivalente.

---

## Build e Deploy

| Script | Função |
|--------|--------|
| `scripts/build-deb-local.sh` | Cross-compila `.deb` arm64 + amd64 via Docker |
| `scripts/build-linux-local.sh` | Build Linux (interno) |
| `scripts/build-orange-pi-image.sh` | Imagem SD para Orange Pi |
| `scripts/flash-sd.sh` | Flasha SD card |
| `scripts/coverage.sh` | Relatório HTML de cobertura |
| `scripts/package-macos.sh` | Empacota macOS |
| `scripts/build-lib.sh` | Libs externas |

**Fluxo branch → .deb → Orange Pi:**

```bash
git checkout feature/issue-{N} && git merge origin/develop
./scripts/build-deb-local.sh
scp output/deb/openrig_0.0.0-dev_arm64.deb root@192.168.15.145:/tmp/
ssh root@192.168.15.145 "dpkg -i /tmp/openrig_0.0.0-dev_arm64.deb && systemctl restart openrig.service"
```

**Regras:** NUNCA compilar na placa. Docker Desktop precisa estar rodando. Só arm64 vai pra placa (amd64 é pra x86 Linux).

---

## Testes

- **Cobertura**: `cargo-llvm-cov` (instalar com `cargo install cargo-llvm-cov` + `rustup component add llvm-tools-preview`). Script: `scripts/coverage.sh`. CI: `.github/workflows/test.yml` (informativo, sem gate).
- **Convenções**: `#[cfg(test)] mod tests`, nomes `<behavior>_<scenario>_<expected>` (ex: `validate_project_rejects_empty_chains`). Sem framework externo. Helpers no próprio módulo.
- **Integração com áudio real**: `#[ignore]` (rodar com `cargo test -- --ignored`).
- **DSP nativos**: golden samples com tolerância `1e-4`, processar silêncio/sine, verificar non-NaN.
- **NAM/LV2/IR builds**: `#[ignore]` (assets externos).
- **Registry tests** em block-* crates: iterar TODOS os modelos via registry.
- **Total**: rodar `cargo test --workspace` (~1100 testes).

---

## graphify

Knowledge graph em `graphify-out/`.

- Antes de responder pergunta sobre arquitetura/codebase, ler `graphify-out/GRAPH_REPORT.md`
- Se `graphify-out/wiki/index.md` existe, navegar lá em vez de raw files
- Cross-module ("como X relaciona com Y"): preferir `graphify query`, `graphify path`, `graphify explain` em vez de grep
- Após modificar código, rodar `graphify update .` (AST-only, sem custo de API)
