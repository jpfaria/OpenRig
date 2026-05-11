# Organização de arquivos (issue #194)

God-files surgem quando lógica feature-specific entra em arquivos compartilhados. Regra dura:

> **Código compartilhado SÓ quando 2+ features usam aquele código.** Lógica feature-specific mora no módulo da feature.

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
- `.rs` test: ilimitado, mas split se passa de 1000 LOC e cobre múltiplas responsabilidades
- `.slint`: **500 LOC**
- `lib.rs` / `mod.rs`: só re-exports, < 100 LOC

## LV2 plugin — `audio_mode` vs builder (issue #130)

Builder e `audio_mode` precisam bater. Misturar = SIGSEGV ou desperdício de CPU.

| Plugin é... | Builder | `ModelAudioMode` |
|---|---|---|
| 1 in / 1 out | `lv2::build_lv2_processor*` com `[in], [out]` | `DualMono` ou `MonoOnly` |
| 1 in / 2 out | `lv2::build_lv2_processor*` com `[in], [L, R]` | `MonoToStereo` |
| 2 in / 2 out | `lv2::build_stereo_lv2_processor*` | `TrueStereo` |

Sintoma clássico: 4 portas declarado `DualMono` → 2 portas dangling → SIGSEGV no primeiro write. Confirmar port count via TTL antes de escolher.
