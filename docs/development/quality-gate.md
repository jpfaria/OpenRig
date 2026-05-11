# Quality Gate — OpenRig

Gate **único** comparativo. Roda local e em CI com mesmo comando, mesmo comportamento. Falha apenas quando o PR piora alguma métrica em relação a `develop`. Dívida preexistente nunca bloqueia.

> Issues #404 (criação) e #410 (unificação).

## TL;DR

```bash
./scripts/qa.sh
```

Local: extrai `origin/develop` em `/tmp/qa-baseline` e compara. Se vermelho → arruma o que regrediu → roda de novo → só então `git push`.

## Filosofia

O gate quebra quando o PR **piora** o projeto. Não importa quanta dívida preexistente exista — se o PR não a aumenta, passa. Cada PR pode reduzir dívida; nunca aumentar.

## Métricas comparadas (PR vs base)

| # | Métrica | Como conta | Falha se… |
|---|---|---|---|
| 1 | fmt | `cargo fmt --check` (linhas `Diff in `) | PR > base |
| 2 | clippy | `cargo clippy -D warnings` sem complexity (linhas `^error`) | PR > base |
| 3 | build | `cargo build --workspace --all-targets` (linhas `^error`) | PR > base |
| 4 | test | `cargo test --no-fail-fast` (soma de `failed: N`) | PR > base |
| 5 | complexity | `clippy -A all -W cognitive_complexity/too_many_lines/too_many_arguments/type_complexity` | PR > base |
| 6 | coverage | `cargo llvm-cov --json` → `lines.percent` | PR < base − `QA_COV_MARGIN` |

Thresholds de complexidade em `clippy.toml` (cognitive 15, lines 80, args 5, type 250) — usados como referência pra contagem, não como bloqueio absoluto.

## Local vs CI — mesmo script

| Aspecto | Local | CI (`.github/workflows/pr.yml`) |
|---|---|---|
| Comando | `./scripts/qa.sh` | `QA_BASELINE_DIR=baseline ./scripts/qa.sh` |
| Baseline | extraído via `git archive origin/develop` em `/tmp/qa-baseline` | reusa checkout que `actions/checkout` faz em `baseline/` |
| Tempo | dois cargos do workspace inteiro (lento) | igual + cache compartilhado entre runs |

## Env vars

| Variável | Default | Uso |
|---|---|---|
| `QA_BASELINE_DIR` | `/tmp/qa-baseline` (auto) | Path de um baseline pronto. Se setado, pula provisionamento. |
| `QA_BASE_REF` | `origin/develop` | Git ref a usar como base. |
| `QA_REFRESH_BASELINE` | `0` | `1` força re-extração do baseline antes de medir. |
| `QA_COV_MARGIN` | `1.0` | Tolerância (pp) pra coverage não regredir. |
| `QA_LOG_DIR` | `target/qa-logs` | Logs por etapa. |

## Validação de negócio — testes obrigatórios

Cobertura sozinha não basta. Toda lógica nova ou alterada **exige teste que valide comportamento esperado**, não apenas execute o caminho.

- Bug fix → teste vermelho que reproduz o bug primeiro, fix depois (TDD).
- Feature → teste cobrindo cenário esperado **e** edge cases.
- Refactor que muda comportamento observável → teste antes do refactor.

## Hook git pre-push (recomendado)

```bash
cat > .git/hooks/pre-push <<'EOF'
#!/usr/bin/env bash
exec ./scripts/qa.sh
EOF
chmod +x .git/hooks/pre-push
```

(Hook é local; cada dev opta.)

## CI — `.github/workflows/pr.yml`

`on: pull_request → develop`. Faz dois checkouts (PR head + base.sha), instala toolchain/deps, e roda `qa.sh`. Em failure: sticky comment com diagnóstico + `gh pr review --request-changes` formal pelo `github-actions[bot]`. Em success: comment ✅ + dismissal automático.

## Forbidden

Pra silenciar o gate sem fix real:

- Subir thresholds em `clippy.toml`.
- Marcar testes como `#[ignore]`.
- `#[allow(clippy::...)]` sem causa raiz justificada.
- `--no-verify` no commit.

A regra: **causa raiz ou escalar**.
