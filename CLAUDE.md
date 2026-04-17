# OpenRig — Contexto do Projeto para Claude Code

## OBRIGATORIO — Superpowers

**REQUIRED SUB-SKILL: Invoke `superpowers:using-superpowers` before ANY action in this project.**

Isso se aplica a todos os agentes — locais e GitHub Actions. Sem exceção.

---

## O que é o OpenRig

Pedalboard/rig virtual para guitarra em Rust. Processa áudio em cadeia (chain) com blocos (blocks) de efeitos e amplificadores. Tem interface gráfica em Slint.

---

## Fluxo de Desenvolvimento — Gitflow (OBRIGATORIO)

Este projeto segue [Gitflow](https://nvie.com/posts/a-successful-git-branching-model/). Sem excecoes.

```
Issue → Branch (from develop) → Commits → PR → Review/Merge
```

### Branches

| Branch | Proposito | Merge into |
|--------|-----------|------------|
| `main` | Releases prontas para producao | — |
| `develop` | Integracao para proxima release | `main` |
| `feature/*` | Novas funcionalidades | `develop` |
| `bugfix/*` | Correcoes de bugs | `develop` |
| `hotfix/*` | Correcoes urgentes em producao | `main` + `develop` |
| `release/*` | Preparacao de release | `main` + `develop` |

### Fluxo

1. **Issue primeiro** — criar issue no GitHub antes de escrever qualquer codigo
2. **Branch por issue desde develop** — `git checkout -b feature/issue-{N}` ou `bugfix/issue-{N}` (NUNCA adicionar sufixo descritivo)
3. **Commits em ingles** — sem `Co-Authored-By`, foco no "why"
4. **PR para develop** — `gh pr create --base develop` com `Closes #N` no body
5. **Merge policy**:
   - **Bugfix/Hotfix**: merge imediato apos criar o PR
   - **Feature/Enhancement**: PR aguarda review antes de merge

### UMA branch por issue (OBRIGATORIO)

**NUNCA criar uma segunda branch para a mesma issue.** Antes de criar qualquer branch:

```bash
# SEMPRE verificar se ja existe branch para a issue
git fetch origin
git branch -a | grep "issue-{N}"
```

- Se **ja existe** → usar a branch existente (`git checkout feature/issue-{N}`)
- Se **nao existe** → criar a branch (`git checkout -b feature/issue-{N}`)
- **NUNCA** adicionar sufixos descritivos como `-parameter-layout`, `-add-captures`, `-fix-something`, `-20260401-1742`, `-v2`, `-fix` etc.
- Se precisar recomecar, resetar a branch existente em vez de criar outra

### Agents paralelos com workspace isolado (.solvers/)

**NUNCA implementar código diretamente no workspace principal.** Cada agent trabalha numa cópia isolada do repo dentro de `.solvers/`:

```
OpenRig/                              ← workspace principal (NÃO editar código aqui)
  .solvers/
    issue-{N}/                        ← cópia isolada do repo para o agent
    doc/                              ← cópia para documentacao (branch main)
```

Regras:
- **Um agent por cópia** — nunca compartilhar `.solvers/issue-{N}/`
- **Uma branch por issue** — nunca misturar issues, nunca criar segunda branch
- **Sempre a partir de develop** — criar branch do develop atualizado
- **PR quando terminar** — push + PR para develop
- **Documentacao vai direto na main** — usar `.solvers/doc/` na branch main
- **Leitura/exploração no workspace principal é OK** — só não editar código
- **Sempre enviar comando de checkout** — ao finalizar trabalho numa branch, incluir o comando `git checkout <branch> && git pull` na resposta para o usuário copiar e testar

Criar workspace isolado:
```bash
# Para codigo (issues)
rsync -a --exclude='target' --exclude='.solvers' . .solvers/issue-{N}/
cd .solvers/issue-{N}
git fetch origin
# Verificar se branch ja existe:
git branch -a | grep "issue-{N}"
# Se existe: git checkout feature/issue-{N}
# Se nao existe: git checkout develop && git pull origin develop && git checkout -b feature/issue-{N}

# Para documentacao
rsync -a --exclude='target' --exclude='.solvers' . .solvers/doc/
cd .solvers/doc
git checkout main && git pull origin main
```

### Regras de codigo

- **Zero warnings** — `cargo build` nao pode ter nenhum warning
- **Zero acoplamento** — blocos nao referenciam modelos, brands ou effect types especificos
- **Single source of truth** — constantes definidas uma vez, nunca duplicadas
- **Separacao de concerns** — crates de business logic nao tem config visual/UI

Ver `CONTRIBUTING.md` para detalhes completos.

### Regras de git

- **NUNCA rebase** — sempre usar `git merge`, nunca `git rebase` ou `git pull --rebase`
- **NUNCA fechar issues** — so fechar quando o usuario pedir explicitamente
- **NUNCA editar workspace principal** — todo codigo vai em `.solvers/issue-{N}/`, a pasta principal e so leitura
- **NUNCA sugerir `cd .solvers/`** — o usuario trabalha no workspace principal. Apos push, sugerir apenas `git checkout <branch> && git pull`
- **Branch sem sufixo** — `feature/issue-{N}` ou `bugfix/issue-{N}`, NUNCA com sufixo descritivo
- **Develop tem prioridade em conflitos** — ao mergear develop na feature branch, usar `git merge -X theirs origin/develop`

### Rastreabilidade — comentarios obrigatorios na issue (OBRIGATORIO)

Todo agent (local ou GitHub) DEVE comentar na issue em dois momentos:

1. **Antes de comecar** — postar um comentario com o plano: o que pretende mudar e por que
2. **Apos terminar** — postar um comentario com o que foi feito: arquivos alterados, decisoes tomadas, qualquer informacao relevante para rastreio futuro

A issue e a fonte da verdade para todas as alteracoes de codigo. Sem esse rastreio, o historico de decisoes se perde.

### Premissa de distribuicao (OBRIGATORIO)

OpenRig e um produto para distribuir em **macOS, Windows e Linux**. Toda decisao deve considerar isso:

- **NUNCA hardcodar paths** — nenhum path absoluto ou relativo hardcoded no codigo
- **NUNCA assumir ambiente de dev** — o codigo roda na maquina do usuario final, nao na do desenvolvedor
- **Paths de assets via config central** — LV2 libs, LV2 bundles, NAM captures, IR captures, tudo vem de config
- **Paths por plataforma** — macOS (`~/Library/Application Support/OpenRig/`), Windows (`%APPDATA%\OpenRig\`), Linux (`~/.local/share/openrig/`)
- **Teste mental obrigatorio** — antes de qualquer decisao, pergunte: "isso funciona se o usuario instalar no Windows?" Se nao, nao faca

---

## O Produto (visão do usuário)

OpenRig é um pedalboard virtual para músicos. O usuário monta sua cadeia de efeitos visualmente, ajusta parâmetros em tempo real e toca com áudio profissional.

### Telas principais

- **Launcher** — criar/abrir projetos, projetos recentes
- **Project Setup** — tela intermediária ao criar novo projeto; pede o nome antes de abrir a view principal
- **Chains** — visualização da cadeia de blocos (pedalboard), arrastar/reordenar blocos
- **Block Editor** — editar parâmetros de um bloco (knobs, sliders, switches)
- **Compact Chain View** — visão compacta com power switches e troca rápida de modelo
- **Settings** — dispositivos de áudio (input/output), sample rate, buffer size
- **Chain Editor** — nome da chain, instrumento, I/O blocks (Input/Output como blocos na cadeia)

**Tamanhos de janela:** Janela principal iniciada em 1100×620px (lógicos) para caber em telas de ~1300×700px (notebooks Windows). Tamanhos mínimos no Slint permitem redimensionamento livre.

### Argumentos de linha de comando e variáveis de ambiente (adapter-gui)

| Argumento / Variável | Exemplo | Efeito |
|---|---|---|
| Caminho do projeto (posicional) | `openrig /path/to/project.yaml` | Abre o projeto direto, pula o launcher |
| `OPENRIG_PROJECT_PATH` | `OPENRIG_PROJECT_PATH=/path/project.yaml openrig` | Equivalente ao caminho posicional (env var tem menor prioridade) |
| `--auto-save` | `openrig --auto-save` | Salva a cada alteração, esconde botão salvar |
| `OPENRIG_AUTO_SAVE` | `OPENRIG_AUTO_SAVE=1 openrig` | Equivalente a `--auto-save` (aceita `1` ou `true`) |
| Combinado | `openrig /path/project.yaml --auto-save` | Ambos os comportamentos |

**Implementação:** Parsing em `crates/adapter-gui/src/main.rs` via `parse_cli_args_from()` (em `lib.rs`). Env vars resolvidas em `main.rs` após o parsing. Auto-save em `sync_project_dirty()` — único ponto de mutação do projeto. Botão salvar condicional via propriedade Slint `auto-save` em `ProjectChainsPage`.

### Tipos de bloco e para que servem

| Tipo | O que faz | Total | Modelos (resumo) |
|------|-----------|-------|-----------------|
| **Preamp** | Pré-amplificação, gain e EQ do amp | 26 | American Clean, Brit Crunch, Modern High Gain (native); JCM 800 2203, Diezel VH4, Thunder 50 (ENGL), '57 Custom Champ/'57 Custom Deluxe/Frontman 15G/PA100 (Fender), Bantamp Meteor (Joyo), AVT50H/YJM100 (Marshall), Mark III (Mesa), Micro Terror (Orange), Shaman (Panama), Classic 30 (Peavey), MIG-100 KT88 (Sovtek), VX Kraken (Victory), MIG-50/22 Caliber (Electro-Harmonix), Blues Baby 22 (Award-Session), Fly (Blackstar), Multitone 50 (Koch), L2 (Lab Series), Lunchbox Jr (ZT) (NAM) |
| **Amp** | Amplificador completo (preamp + power amp + cab) | 29 | Blackface Clean, Tweed Breakup, Chime (native); Bogner Ecstasy/Shiva, Dumble ODS, EVH 5150, Friedman BE100 Deluxe, Marshall JCM800/JVM/JMP-1 Head/JMP-1, Mesa Mark V/Rectifier, Peavey 5150, Ampeg SVT Classic, Dover DA-50+Mesa, Fender Bassman 1971/Deluxe Reverb '65/Super Reverb 1977, Marshall Super 100 1966, Peavey 5150+Mesa 4x12, Roland JC-120B, Synergy DRECT+Mesa, Vox AC30/'61 Fawn (NAM); GxBlueAmp, GxSupersonic, MDA Combo (LV2) |
| **Cab** | Simulação de caixa/falante | 17 | American 2x12, Brit 4x12, Vintage 1x12 (native); Celestion Cream 4x12, Evil Chug (Blackstar+PRS), Fender Deluxe Reverb Oxford, G12M Greenback 2x12, G12M Greenback Multi-Mic, G12T-75 4x12, Marshall 4x12 V30, Mesa OS 4x12 V30, Mesa Standard 4x12 V30, Roland JC-120, V30 4x12, Vox AC30 Blue, Vox AC50 2x12 Goodmans (IR); GxUltraCab (LV2) |
| **Gain** | Overdrive, distorção, fuzz, boost | 91 | TS9 (native); Boss DS-1/HM-2/FZ-1W/MT-2/BD-2, Klon Centaur, RAT/RAT2, OCD, OD808, TS808, Darkglass Alpha Omega/B7K, JHS Bonsai, Bluesbreaker, Vemuram Jan Ray + 34 outros (NAM); Guitarix ×40, CAPS Spice/X2, OJD, Wolf Shaper, MDA + outros (LV2) |
| **Delay** | Eco e repetição temporal | 14 | Analog Warm, Digital Clean, Slapback, Reverse, Modulated, Tape Vintage (native); MDA DubDelay, TAP Doubler/Echo/Reflector, Bollie, Avocado, Floaty, Modulay (LV2) |
| **Reverb** | Ambiência e simulação de espaço | 19 | Hall, Plate Foundation, Room, Spring (native); Dragonfly Hall/Room/Plate/Early, CAPS Plate/X2/Scape, TAP Reflector/Reverberator, MDA Ambience, MVerb, B Reverb, Roomy, Shiroverb, Floaty (LV2) |
| **Modulation** | Chorus, flanger, tremolo, vibrato | 16 | Classic/Stereo/Ensemble Chorus, Sine Tremolo, Vibrato (native); TAP Chorus/Flanger/Tremolo/Rotary, MDA Leslie/RingMod/ThruZero, FOMP CS Chorus/Phaser, CAPS Phaser II, Harmless, Larynx (LV2) |
| **Dynamics** | Compressor e gate | 9 | Studio Clean Compressor, Noise Gate, Brick Wall Limiter (native); TAP DeEsser/Dynamics/Limiter, ZamComp, ZamGate, ZaMultiComp (LV2) |
| **Filter** | EQ e moldagem tonal | 13 | Three Band EQ, Guitar EQ, 8-Band Parametric EQ (native); TAP Equalizer/BW, ZamEQ2, ZamGEQ31, CAPS AutoFilter, FOMP Auto-Wah, MOD HPF/LPF, Filta, Mud (LV2) |
| **Wah** | Pedal wah-wah | 2 | Cry Classic (native); GxQuack (LV2) |
| **Utility** | Ferramentas | 2 | Chromatic Tuner, Spectrum Analyzer (native) |
| **Body** | Ressonância de corpo acústico | 114 | Martin (45), Taylor (30), Gibson (10), Yamaha (5), Guild (4), Takamine (4), Cort (4), Emerald (2), Rainsong (2), Lowden (2) + outros boutique (IR) |
| **Pitch** | Pitch shifting e harmonização | 4 | Harmonizer, x42 Autotune, MDA Detune, MDA RePsycho (LV2) |
| **Full Rig** | Rig completo com pedais + amp + cab | 0 | (reservado para capturas com cadeia completa incluindo pedais) |
| **IR** | Loader genérico de IR | 1 | generic_ir |
| **NAM** | Loader genérico de NAM | 1 | generic_nam |
| **Input** | Entrada de áudio (device + channels) | — | standard |
| **Output** | Saída de áudio (device + channels) | — | standard |
| **Insert** | Loop de efeito externo (send/return) | — | external_loop |

**Total: 362+ modelos em 16 tipos de bloco processadores (5 backends: Native 35, NAM 89, IR 127, LV2 105, VST3 6).**

### Parâmetros comuns

- **Preamp/Amp nativos**: input, gain, bass, middle, treble, presence, depth, sag, master, bright
- **NAM preamp**: volume (50-70%), gain (10-100%) em steps
- **Delay**: time_ms (1-2000ms), feedback (0-100%), mix (0-100%)
- **Reverb**: room_size, damping, mix (0-100%)
- **Compressor**: threshold, ratio, attack_ms, release_ms, makeup_gain, mix
- **Gate**: threshold, attack_ms, release_ms
- **EQ (Three Band / Guitar EQ)**: low, mid, high (0-100% → -24dB a +24dB)
- **8-Band Parametric EQ** (`eq_eight_band_parametric`): por banda — `band{N}_enabled` (bool), `band{N}_type` (peak/low_shelf/high_shelf/low_pass/high_pass/notch), `band{N}_freq` (20–20000 Hz), `band{N}_gain` (-24/+24 dB), `band{N}_q` (0.1–10). Freqs padrão: 62/125/250/500/1k/2k/4k/8kHz. Suporta todos os instrumentos. DualMono.
- **Gain pedals**: drive, tone, level
- **Volume**: volume (0-100%), mute (on/off)
- **Tuner**: reference_hz (400-480Hz, default 440)
- **Vibrato**: rate_hz (0.1-8.0Hz), depth (0-100%) — 100% wet, no dry signal
- **Autotune Chromatic**: speed (0-100ms), mix (0-100%), detune (±50 cents), sensitivity (0-100%)
- **Autotune Scale**: speed, mix, detune, sensitivity + key (C-B), scale (Major, Minor, Pentatonic Maj/Min, Harmonic Minor, Melodic Minor, Blues, Dorian)

### Backends de áudio

- **Native** — DSP em Rust, mais rápido, menor CPU
- **NAM** — Neural Amp Modeler, captura realista de amps/pedais
- **IR** — Impulse Response, convolução para cabs e corpos acústicos
- **LV2** — Plugins externos open-source

### Instrumentos suportados

electric_guitar, acoustic_guitar, bass, voice, keys, drums, generic

Cada chain tem um instrumento que filtra quais blocos podem ser adicionados.

### Configuração de áudio — I/O como blocos

Input, Output e Insert são variantes de `AudioBlockKind` (`InputBlock`, `OutputBlock`, `InsertBlock`) dentro de `chain.blocks`. Não existem listas separadas `Chain.inputs` / `Chain.outputs`.

- **Primeiro bloco** da chain é sempre um Input (fixo, não removível)
- **Último bloco** da chain é sempre um Output (fixo, não removível)
- Blocos extras de Input/Output/Insert podem ser adicionados no meio da chain
- Cada Input cria um stream paralelo isolado (instância independente da cadeia de blocos)
- Output é um tap não-destrutivo: copia o sinal sem interromper o fluxo
- Insert divide a chain em segmentos — cada segmento tem seus próprios effect blocks e output routes. Quando desabilitado, o sinal passa direto (bypass).

#### Estrutura dos blocos I/O

- **InputBlock**: `model: String` (default "standard"), `entries: Vec<InputEntry>`
  - Cada `InputEntry` tem: `name`, `device_id`, `mode` (Mono/Stereo/DualMono), `channels`
- **OutputBlock**: `model: String` (default "standard"), `entries: Vec<OutputEntry>`
  - Cada `OutputEntry` tem: `name`, `device_id`, `mode` (Mono/Stereo), `channels`
- **InsertBlock**: `model: String`, `send: InsertEndpoint`, `return_: InsertEndpoint`
  - Cada `InsertEndpoint` tem: `device_id`, `mode`, `channels`
- **Nota**: `name` fica nas entries, não no InputBlock/OutputBlock

#### Configurações gerais

- Devices: input e output independentes (podem ser devices diferentes)
- Sample rates: 44.1kHz, 48kHz, 88.2kHz, 96kHz
- Buffer sizes: 32, 64, 128, 256, 512, 1024 samples
- Bit depths: 16, 24, 32 bits
- **YAML (novo formato)**: todos os blocos I/O ficam inline no array `blocks:` (sem seções `inputs:`/`outputs:` separadas)
- **YAML (formato antigo)**: seções `inputs:` / `outputs:` separadas ainda são suportadas por backward compatibility — na deserialização tudo é reunido no vetor `blocks`
- **Migração**: YAML antigo com `input_device_id`/`output_device_id` (campos únicos) é migrado automaticamente para o formato novo ao carregar

#### Per-machine device settings (gui-settings.yaml)

Device settings (sample rate, buffer size, bit depth) são **per-machine**, não per-project. Ficam em `gui-settings.yaml` no diretório de configuração do OS:
- macOS: `~/Library/Application Support/OpenRig/gui-settings.yaml`
- Windows: `%APPDATA%\OpenRig\gui-settings.yaml`
- Linux: `~/.config/OpenRig/gui-settings.yaml`

**Fluxo:**
1. `load_project_session()` lê `gui-settings.yaml` e popula `project.device_settings` em memória
2. Settings UI lê/grava de `gui-settings.yaml` via `FilesystemStorage`
3. `infra-cpal` lê `project.device_settings` (já populado) para resolver devices
4. YAML do projeto **não persiste** `device_settings` (campo tem `skip_serializing`)
5. YAML antigo com `device_settings` ainda deserializa (backward compat)

#### JACK lifecycle management (Linux)

No Linux com feature `jack`, o OpenRig controla o ciclo de vida do JACK:

- **Auto-launch**: quando uma chain é habilitada e JACK não está rodando, `ensure_jack_running()` em infra-cpal:
  1. Detecta a placa USB audio via `/proc/asound/cards`
  2. Lê sample_rate e buffer_size do `project.device_settings` (gui-settings.yaml)
  3. Configura mixer ALSA (Mic 46%, PCM 100% = unity gain)
  4. Lança `jackd -d alsa -d hw:$CARD -r $SR -p $BUF -n 3` como processo background
  5. Espera até 5s pelo socket JACK aparecer em `/dev/shm/`
- **Auto-reconnect**: timer de 2s no adapter-gui (`health_timer`) verifica `is_healthy()`:
  - Se JACK caiu (USB desconectou, service reiniciou) → mostra "Audio device disconnected"
  - Tenta `try_reconnect()` a cada 2s → quando JACK volta, reconecta chains automaticamente
  - Mostra "Audio device reconnected" quando sucesso
- **Sem impacto em macOS/Windows**: `ensure_jack_running()` e `is_healthy()` são `#[cfg(all(target_os = "linux", feature = "jack"))]`

---

## Arquitetura

### Crates principais

- **`crates/block-preamp/`** — Bloco de pré-amplificador (preamp). Contém modelos NAM e nativos.
- **`crates/block-amp/`** — Bloco de amplificador completo (preamp + cab). Contém modelos nativos e NAM.
- **`crates/adapter-gui/`** — Interface gráfica em Slint (`.slint` files em `ui/`).
- **`crates/block-core/`** — Tipos base: `BlockProcessor`, `AudioChannelLayout`, `ParameterSet`, etc.
- **`crates/nam/`** — Integração com Neural Amp Modeler.
- **`crates/asset-runtime/`** — `EmbeddedAsset`, `materialize()` para assets compilados no binário.

### Assets

```
assets/
  brands/
    marshall/logo.svg   <- logo oficial worldvectorlogo, fill="currentColor"
    vox/logo.svg        <- logo oficial worldvectorlogo, cores #53ad99 (teal) + #d99346 (gold)
    native/             <- (vazio, sem marca real)
  amps/
    marshall/jcm800-2203/
      controls.svg      <- painel completo (fundo escuro, secoes, knobs como circulos)
      component.yaml    <- APENAS assets paths + svg_cx/cy dos controles
    native/
      american-clean/controls.svg + component.yaml
      brit-crunch/controls.svg + component.yaml
      modern-high-gain/controls.svg + component.yaml
    vox/ac30/
      controls.svg      <- padrao de referencia (AC30 e o template visual)
      amp.svg
      component.yaml    <- ainda tem brand/model/etc pois nao tem struct Rust
    generic/component.yaml
```

---

## Regras importantes

### Brand/type ficam no Rust, nao no YAML

`PreampModelDefinition` (em `crates/block-preamp/src/registry.rs`) tem:
```rust
pub struct PreampModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,   // ex: "Marshall JCM 800 2203"
    pub brand: &'static str,          // ex: "marshall", "vox", "native"
    pub backend_kind: PreampBackendKind,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub validate: fn(&ParameterSet) -> Result<()>,
    pub asset_summary: fn(&ParameterSet) -> Result<String>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
}
```

Os `component.yaml` so tem: caminhos de assets e posicoes SVG (`svg_cx`, `svg_cy`) dos controles — para uso futuro no overlay de knobs.

### Funcoes publicas em lib.rs

```rust
pub fn preamp_display_name(model: &str) -> Result<&'static str>
pub fn preamp_brand(model: &str) -> Result<&'static str>
pub fn preamp_type_label(model: &str) -> Result<&'static str>  // "native" | "NAM" | "IR"
```

### Logos de marcas

- **Sempre buscar em `cdn.worldvectorlogo.com`** — nao desenhar a mao.
- Remover o fundo branco/preto do SVG original.
- Marshall: usar `fill="currentColor"` para theming.
- Vox: cores fixas `#53ad99` e `#d99346` (ja sao coloridos).
- **Nao colocar a logo do brand dentro da imagem do equipamento.**

### Padrao de controls.svg (seguir o AC30)

`controls.svg` E o painel completo — nao criar `panel.svg` separado. Estrutura:
```svg
<svg viewBox="0 0 800 200" width="800" height="200">
  <!-- gradiente escuro de fundo -->
  <rect fill="url(#panel)"/>
  <!-- rotulo do modelo (esquerda) -->
  <!-- linha tracejada separando secoes -->
  <!-- rotulos de secao no topo -->
  <!-- circulos como ancoras de knob: fill="#111" stroke="#505050" stroke-width="1.5" -->
  <!-- texto de label abaixo de cada circulo -->
</svg>
```

Controles nao editaveis (ex: EQ fixo no Marshall NAM) -> mostrar com `opacity="0.6"`, sem `id`.
Controles editaveis -> tem `id="ctrl-xxx"` para overlay futuro.

### Registry auto-gerado (build.rs)

`crates/block-preamp/build.rs` escaneia `src/*.rs` procurando `MODEL_DEFINITION` e gera `generated_registry.rs` com array `MODEL_DEFINITIONS`. Ao criar novo modelo, basta criar o `.rs` com `pub const MODEL_DEFINITION: PreampModelDefinition = ...`.

---

## Modelos de preamp existentes

| ID | Display Name | Brand | Backend |
|----|-------------|-------|---------|
| `american_clean` | American Clean | native | Native |
| `brit_crunch` | Brit Crunch | native | Native |
| `modern_high_gain` | Modern High Gain | native | Native |
| `marshall_jcm_800_2203` | Marshall JCM 800 2203 | marshall | NAM |
| `nam_engl_thunder_50` | Thunder 50 | engl | NAM |
| `nam_fender_57_champ` | '57 Custom Champ | fender | NAM |
| `nam_fender_57_deluxe` | '57 Custom Deluxe | fender | NAM |
| `nam_fender_frontman_15g` | Frontman 15G | fender | NAM |
| `nam_joyo_bantamp_meteor` | Bantamp Meteor | joyo | NAM |
| `nam_marshall_avt50h` | AVT50H | marshall | NAM |
| `nam_marshall_yjm100` | YJM100 | marshall | NAM |
| `nam_mesa_mark_iii` | Mark III | mesa | NAM |
| `nam_orange_micro_terror` | Micro Terror | orange | NAM |
| `nam_panama_shaman` | Shaman | panama | NAM |
| `nam_peavey_classic_30` | Classic 30 | peavey | NAM |
| `nam_sovtek_mig100` | MIG-100 KT88 | sovtek | NAM |
| `nam_victory_vx_kraken` | VX Kraken | victory | NAM |
| `nam_ehx_mig50` | MIG-50 | electro-harmonix | NAM |
| `nam_ehx_22_caliber` | 22 Caliber | electro-harmonix | NAM |
| `nam_award_session_blues_baby_22` | Blues Baby 22 | award-session | NAM |
| `nam_blackstar_fly` | Fly | blackstar | NAM |
| `nam_fender_pa100` | PA100 | fender | NAM |
| `nam_koch_multitone_50` | Multitone 50 | koch | NAM |
| `nam_lab_series_l2` | L2 | lab-series | NAM |
| `nam_zt_lunchbox_jr` | Lunchbox Jr | zt | NAM |

Os 3 nativos usam `native_core::model_schema()` -> mesmos parametros: gain, bass, middle, treble, presence, depth, sag, master, bright.

Marshall NAM e todos os NAM novos -> parametros: volume (50-70%), gain (10-100%), em steps de 10 (mapeado para captures .nam).

---

## Interface Grafica (Slint)

### Arquivos principais
- `crates/adapter-gui/ui/app-window.slint` — janela principal, 520px width para o BlockEditorWindow
- `crates/adapter-gui/ui/pages/project_chains.slint` — pagina de chains, contem `BlockEditorPanel`

### BlockEditorPanel — redesign do editor de blocos

Quando o bloco selecionado e `preamp`, mostrar a imagem do painel (`controls.svg`) em vez de so sliders.

Propriedades computadas:
```slint
property <bool> is-preamp:
    root.block-drawer-selected-type-index >= 0
    && root.block-drawer-selected-type-index < root.block-type-options.length
    && root.block-type-options[root.block-drawer-selected-type-index].icon_kind == "preamp";

property <string> selected-model-id:
    root.block-drawer-selected-model-index >= 0
    && root.block-drawer-selected-model-index < root.block-model-options.length
    ? root.block-model-options[root.block-drawer-selected-model-index].model_id
    : "";
```

Imagem do painel (ternary chain porque `@image-url()` precisa ser compile-time):
```slint
if root.is-preamp : Rectangle {
    x: 8px; y: 132px;
    width: parent.width - 16px;
    height: (parent.width - 16px) / 4;  // aspect ratio 4:1
    border-radius: 6px; clip: true;
    Image {
        source: root.selected-model-id == "marshall_jcm_800_2203"
            ? @image-url("caminho/marshall/jcm800-2203/controls.svg")
            : root.selected-model-id == "american_clean"
            ? @image-url("caminho/native/american-clean/controls.svg")
            : ...
            : @image-url("caminho/generic/controls.svg");
        image-fit: fill;
    }
}
```

---

## Tipos de Instrumento

Cada chain tem um `instrument` que filtra quais blocos podem ser adicionados.

### Valores validos

`electric_guitar` | `acoustic_guitar` | `bass` | `voice` | `keys` | `drums` | `generic`

`generic` = sem filtragem, mostra todos os blocos.

### Constantes (em `crates/block-core/src/lib.rs`)

```rust
pub const INST_ELECTRIC_GUITAR: &str = "electric_guitar";
pub const INST_ACOUSTIC_GUITAR: &str = "acoustic_guitar";
pub const INST_BASS:             &str = "bass";
pub const INST_VOICE:            &str = "voice";
pub const INST_KEYS:             &str = "keys";
pub const INST_DRUMS:            &str = "drums";

pub const ALL_INSTRUMENTS:        &[&str] = &[/* todos acima */];
pub const GUITAR_BASS:            &[&str] = &[INST_ELECTRIC_GUITAR, INST_BASS];
pub const GUITAR_ACOUSTIC_BASS:   &[&str] = &[INST_ELECTRIC_GUITAR, INST_ACOUSTIC_GUITAR, INST_BASS];
```

### Suporte por modelo

Cada `MODEL_DEFINITION` declara `supported_instruments: &[&str]`. O `adapter-gui` filtra a lista de blocos disponiveis usando esse campo ao adicionar blocos a uma chain.

Exemplos de uso tipico:
- Preamps/amps: `GUITAR_ACOUSTIC_BASS`
- Amps/cabs com distorcao: `GUITAR_BASS`
- Efeitos universais (reverb, delay): `ALL_INSTRUMENTS`

### Persistencia

O campo `instrument` e salvo no YAML da chain. Valor padrao (retrocompatibilidade): `electric_guitar`. O instrumento e definido na criacao da chain e nao pode ser alterado depois.

```yaml
chains:
  - description: guitar 1
    instrument: electric_guitar
    blocks:
      - type: input
        model: standard
        enabled: true
        entries:
          - name: Input 1
            device_id: "coreaudio:..."
            mode: mono
            channels: [0]
      - type: preamp
        model: marshall_jcm_800_2203
        enabled: true
        params:
          volume: 70.0
          gain: 40.0
      - type: insert
        model: external_loop
        enabled: true
        send:
          device_id: "coreaudio:send_dev"
          mode: stereo
          channels: [0, 1]
        return_:
          device_id: "coreaudio:return_dev"
          mode: stereo
          channels: [0, 1]
      - type: delay
        model: digital_clean
        enabled: true
        params:
          time_ms: 350.0
          feedback: 40.0
          mix: 30.0
      - type: output
        model: standard
        enabled: true
        entries:
          - name: Output 1
            device_id: "coreaudio:..."
            mode: stereo
            channels: [0, 1]
```

Internamente, a Chain tem um único vetor `blocks: Vec<AudioBlock>` onde:
- `blocks[0]` = InputBlock (fixo)
- `blocks[1..N-1]` = blocos de efeito (Nam, Core, Select) e opcionais (Insert, I/O extras)
- `blocks[N-1]` = OutputBlock (fixo)

O Insert divide a chain em segmentos. Cada segmento tem sua própria lista de effect blocks e output routes. Um Insert desabilitado funciona como bypass (sinal passa direto).

---

## Build e Deploy

### Scripts principais

| Script | O que faz |
|--------|-----------|
| `scripts/build-deb-local.sh` | Cross-compila .deb para arm64 + amd64 via Docker no Mac |
| `scripts/build-linux-local.sh` | Build Linux (chamado pelo build-deb-local.sh) |
| `scripts/build-orange-pi-image.sh` | Gera imagem SD completa para Orange Pi |
| `scripts/flash-sd.sh` | Flasha imagem no SD card |
| `scripts/coverage.sh` | Gera relatório HTML de cobertura em `coverage/` |
| `scripts/package-macos.sh` | Empacota para macOS |
| `scripts/build-lib.sh` | Build de libs externas |

### Fluxo completo: branch → .deb → Orange Pi

```bash
# 1. Checkout e merge do develop
git checkout feature/issue-{N}
git merge origin/develop

# 2. Build do .deb (requer Docker rodando)
./scripts/build-deb-local.sh
# Output: output/deb/openrig_0.0.0-dev_arm64.deb

# 3. Deploy no Orange Pi (192.168.15.145)
scp output/deb/openrig_0.0.0-dev_arm64.deb root@192.168.15.145:/tmp/
ssh root@192.168.15.145 "dpkg -i /tmp/openrig_0.0.0-dev_arm64.deb && systemctl restart openrig.service"
```

### Regras de build

- **NUNCA compilar na placa** — sempre cross-compile no Mac com `build-deb-local.sh`
- **Docker obrigatorio** — o build usa container arm64; Docker Desktop precisa estar rodando
- **Somente arm64 para Orange Pi** — o arquivo amd64 e para x86 Linux, nao para a placa

---

## Pendencias / Proximos passos

- [ ] **Overlay de knobs sobre controls.svg** — usar `svg_cx`/`svg_cy` do component.yaml para posicionar componentes Slint interativos por cima da imagem
- [ ] **amp no BlockEditorPanel** — o Vox AC30 e `amp`, nao `preamp`; a logica `is-preamp` precisa de equivalente `is-amp`
- [ ] **Logo OpenRig** — `assets/brands/openrig/` esta vazio
- [ ] **output_db nos paineis nativos** — parametro existe no Rust mas nao esta no controls.svg
- [ ] **Vox AC30 -> struct Rust** — ainda nao tem `PreampModelDefinition` equivalente (e amp, arquitetura diferente)
- [ ] **Marshall JCM 800 2203 — versao Native** — criar modelo nativo com `NativeAmpHeadProfile` que expoe todos os controles reais: PRESENCE, BASS, MIDDLE, TREBLE, MASTER VOLUME, PRE-AMP. O painel (`controls.svg`) tera o painel completo igual ao amp real. O NAM atual (`marshall_jcm_800_2203`) continua com apenas MASTER + PRE-AMP.

---

## Testes

### Ferramenta de cobertura

- **`cargo-llvm-cov`** — instalar com `cargo install cargo-llvm-cov` + `rustup component add llvm-tools-preview`
- **Script local**: `scripts/coverage.sh` — gera relatório HTML em `coverage/`
- **CI**: `.github/workflows/test.yml` — roda `cargo test --workspace` + relatório de cobertura (informativo, sem gate)

### Convenções

- Testes dentro do módulo: `#[cfg(test)] mod tests { ... }`
- Nomenclatura: `<behavior>_<scenario>_<expected>` (ex: `validate_project_rejects_empty_chains`)
- Sem frameworks externos — usar `assert!`, `assert_eq!`, `assert!(result.is_err())`
- Helpers de teste no próprio módulo — sem crate de test-utils separado
- Testes de integração com áudio real: `#[ignore]` (rodar com `cargo test -- --ignored`)

### DSP

- **Nativos**: golden samples com tolerância `1e-4`, processar silêncio/sine e verificar non-NaN
- **NAM/LV2/IR builds**: `#[ignore]` (dependem de assets externos)
- **Registry tests** para block-* crates: iterar sobre TODOS os modelos via registry (schema, validate, build)

### Cobertura atual (~1100 testes)

| Crate | Testes |
|-------|--------|
| domain | 87 |
| block-core | 134 |
| application | 50 |
| infra-filesystem | 32 |
| engine | 85+ |
| infra-yaml | 57 |
| project | 133+ |
| adapter-gui | 83 |
| block-delay | 31 |
| block-reverb | 25 |
| block-dyn | 39 |
| block-filter | 33+ |
| block-mod | 42 |
| block-wah | 16 |
| block-gain | 12+ |
| block-preamp | 9+ |
| block-amp | 10 |
| block-util | 17 |
| block-pitch | 5 |
| ir | 31 |
| nam | 30 |
| infra-cpal | 12 |
