# Organização de arquivos (issues #194, #873)

God-files surgem quando lógica feature-specific entra em arquivos compartilhados. Regra dura:

> **Código compartilhado SÓ quando 2+ features usam aquele código.** Lógica feature-specific mora no módulo da feature.

## A lei: um arquivo, uma responsabilidade

**Arquivo de produção faz UMA coisa** — uma responsabilidade é um motivo pra mudar. O teste é verbal: descreva o arquivo em uma frase; precisou de "e", são dois arquivos.

**Só arquivo de teste pode ser grande.** Teste não tem cap de linhas. Produção (`.rs` não-test, `.slint`) tem cap E tem a lei da responsabilidade — e a lei é a que manda: 300 linhas fazendo 4 coisas já viola, mesmo passando no cap.

Responsabilidade nova nunca entra no fim de um arquivo existente — nasce no arquivo dela.

### A declaração no cabeçalho

Todo arquivo de produção declara sua responsabilidade na primeira dúzia de linhas:

```rust
//! Responsibility: rebuilds a chain runtime in place
```
```slint
// Responsibility: renders one block tile inside a chain row
```

`validate.sh` (check 1) reprova arquivo tocado que não declara, e reprova declaração que precisa de "and"/"+"/vírgula pra ser escrita — isso é o arquivo dizendo que faz duas coisas. Arquivo legado que você não tocou só avisa: a regra entra sem rewrite do repo, e cada arquivo paga a dele quando alguém o edita.

## Onde mora cada coisa

| Situação | Onde mora |
|---|---|
| Constante/tipo/fn usados por 2+ crates ou 2+ features | crate compartilhado (`block-core`, `domain`, `project`) |
| Lógica de UM modelo (preset Marshall JCM 800, schema do TS9) | crate do efeito dono |
| Visual config (cor, fonte, posição de foto) | `adapter-gui/src/visual_config/` — NUNCA no `MODEL_DEFINITION` |
| Wiring de UM widget Slint | arquivo `*_wiring.rs` próprio |
| Audio thread hot path | crate `engine` (split por responsabilidade) |

## Anti-padrões

```
❌ match/if novo em crate central a cada modelo novo
❌ adapter-gui/src/lib.rs com 9000+ LOC de callbacks Slint
❌ project/src/block.rs com match-branches que crescem por effect_type
❌ visual config dentro de MODEL_DEFINITION (mistura business + GUI)
❌ string literal de model_id em arquivo compartilhado
```

## Padrões corretos

```
✅ cada block-* exporta <crate>_model_visual(id) — UI olha brand sem tocar business
✅ adapter-gui split em *_wiring.rs por feature
✅ engine runtime split por responsabilidade
✅ slint ternary por model_id em UM componente (block_panel_brand_strip.slint) — exceção autorizada
```

## Caps de tamanho (validate.sh)

- `.rs` (não-test): **600 LOC**
- `.slint`: **500 LOC**
- `.rs` de teste: **sem cap** — é a única exceção de tamanho do repo, e `validate.sh` nem mede
- `lib.rs` / `mod.rs`: só re-exports, < 100 LOC

### Catraca do débito (#873)

`validate.sh` mantém `DEBT_FILES` com o LOC de referência de cada arquivo de produção que já nasceu acima do cap. A lista é catraca, não anistia:

| Situação | Resultado do gate |
|---|---|
| Arquivo NOVO acima do cap | ❌ FAIL — split antes de commitar |
| Arquivo em débito que **cresceu** acima do LOC de referência | ❌ FAIL — proibido crescer |
| Arquivo em débito que encolheu, ainda acima do cap | ⚠️ WARN + baixe o LOC de referência no mesmo commit |
| Arquivo em débito que caiu **abaixo** do cap | ❌ FAIL — tire a linha da lista (o débito acabou) |

Nunca se acrescenta arquivo à lista. Ela só encolhe.

### Guard em tempo de edição

O `line-cap-guard` do plugin dev-rules (PreToolUse) **nega Edit/Write que cresça** um arquivo já acima do cap; edit que encolhe passa, então o split nunca fica bloqueado por si mesmo. Os caps que ele usa vêm do `.dev-rules.json` do repo (`line_caps`) — mesma fonte de números do `validate.sh`.

## LV2 plugin — `audio_mode` vs builder (issue #130)

Builder e `audio_mode` precisam bater. Misturar = SIGSEGV ou desperdício de CPU.

| Plugin é... | Builder | `ModelAudioMode` |
|---|---|---|
| 1 in / 1 out | `lv2::build_lv2_processor*` com `[in], [out]` | `DualMono` ou `MonoOnly` |
| 1 in / 2 out | `lv2::build_lv2_processor*` com `[in], [L, R]` | `MonoToStereo` |
| 2 in / 2 out | `lv2::build_stereo_lv2_processor*` | `TrueStereo` |

Sintoma clássico: 4 portas declarado `DualMono` → 2 portas dangling → SIGSEGV no primeiro write. Confirmar port count via TTL antes de escolher.
