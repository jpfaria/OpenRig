# Quality Gate — OpenRig

Gate único de qualidade que roda **localmente antes do push** e **no CI antes do merge**. Mesma fonte de verdade: `scripts/qa.sh`.

> Issue #404 — blindar PRs (humano e agent) contra regressão.

## TL;DR

```bash
./scripts/qa.sh
```

Se falha → arruma → roda de novo até verde → só então `git push`.

## Filosofia: absoluto + comparativo

O gate quebra quando o PR **piora** o projeto, não pelo simples fato do código preexistente já ter dívida.

- **Gate absoluto** (`scripts/qa.sh`, roda local e em CI): checks que NÃO podem regredir e são triviais de manter — fmt, lints (`-D warnings`), build, test, geração de cobertura.
- **Gate comparativo** (`scripts/qa-comparative.sh`, só em CI): mede complexidade e cobertura **no PR e em `develop`**. Falha se PR > base (complexidade) ou PR < base − margem (cobertura).

Resultado: você consegue mergear código mesmo num projeto com dívida atual, desde que não a aumente. Cada PR pode reduzir dívida; nunca pode aumentar.

## O que o gate ABSOLUTO verifica (qa.sh)

| # | Etapa | Comando | Falha se… |
|---|---|---|---|
| 1 | Formatação | `cargo fmt --all --check` | arquivo `.rs` desformatado |
| 2 | Lint | `cargo clippy --workspace --all-targets -- -D warnings` | qualquer warning de clippy |
| 3 | Build | `cargo build --workspace --all-targets` | erro/warning de build |
| 4 | Testes | `cargo test --workspace --all-targets` | qualquer teste vermelho |
| 5 | Cobertura | `cargo llvm-cov --workspace --lcov` (gera lcov.info; `--fail-under-lines` opcional via `QA_MIN_COVERAGE`) | piso explícito violado |

Lints de **complexidade** (cognitive_complexity, too_many_lines, too_many_arguments, type_complexity) ficam **fora** do gate absoluto pra não bloquear por código preexistente. São tratados pelo gate comparativo.

## O que o gate COMPARATIVO verifica (qa-comparative.sh)

Roda só em CI. Faz checkout de `develop` em paralelo e compara:

| Métrica | Como mede | Falha se… |
|---|---|---|
| Complexidade | Conta linhas de erro com clippy ativando os 4 lints de complexidade | PR > base |
| Cobertura | `cargo llvm-cov --json` parsed por `jq` (`.data[0].totals.lines.percent`) | PR < base − `QA_COV_MARGIN` (default 1.0pp) |

Thresholds de complexidade ficam em `clippy.toml` (cognitive 15, lines 80, args 5, type 250) — usados pra contagem comparativa, não como bloqueio absoluto.

## Princípio orientador

- **Gate estrutural** (fmt + clippy + complexidade) → forma do código. Mecânico.
- **Testes** → comportamento e regra de negócio. Única camada que valida lógica.
- **Cobertura** → piso, não teto. Cobertura alta ≠ testes bons.
- **Skills do projeto** → julgamento (invariantes de áudio, idiomatic Rust, decisões de arquitetura). Não duplicam regra mecânica enforçada pelo gate.

## Validação de negócio — testes obrigatórios

Cobertura sozinha não basta. Toda lógica nova ou alterada **exige teste que valide comportamento esperado**, não apenas execute o caminho.

- Bug fix → teste vermelho que reproduz o bug primeiro, fix depois (TDD).
- Feature → teste cobrindo cenário esperado **e** edge cases.
- Refactor que muda comportamento observável → teste antes do refactor.

## Configuração via env vars

| Variável | Default | Uso |
|---|---|---|
| `QA_MIN_COVERAGE` | `0` | Piso de cobertura por linhas. Subir gradualmente conforme baseline calibra. |
| `QA_SKIP_COVERAGE` | `0` | `1` pula cobertura (iteração rápida local; CI **nunca** pula). |
| `QA_LOG_DIR` | `target/qa-logs` | Onde os logs por etapa ficam. |

Exemplos:

```bash
# Iteração rápida durante desenvolvimento (sem coverage):
QA_SKIP_COVERAGE=1 ./scripts/qa.sh

# Apertar piso de coverage:
QA_MIN_COVERAGE=70 ./scripts/qa.sh
```

## Hook git pre-push (recomendado)

Para nunca pushar código vermelho por engano:

```bash
cat > .git/hooks/pre-push <<'EOF'
#!/usr/bin/env bash
exec ./scripts/qa.sh
EOF
chmod +x .git/hooks/pre-push
```

(O hook é local. Não comitamos no repo — cada dev opta.)

## CI — `.github/workflows/pr.yml`

Roda `bash scripts/qa.sh` em `pull_request` para `develop`. Em caso de falha:

1. **Sticky comment** no PR (`header: openrig-quality-gate`) com diagnóstico:
   - Etapas que falharam.
   - Trecho dos logs (`target/qa-logs/*.log`).
   - Link pro run completo.
   - Instrução para rodar `./scripts/qa.sh` localmente até passar.
2. **Request-changes** formal pelo `github-actions[bot]` (negativa oficial, não só "checks failing").
3. Logs uploaded como artifact `qa-logs`.

Quando o próximo push deixa o gate verde:
- Sticky comment é atualizado para ✅.
- Request-changes é dismissed automaticamente.

## Loop de auto-correção do agent

Configurado em `.github/workflows/claude.yml` (`additional_system_prompt`):

1. Após `git push`, agent aguarda CI: `gh pr checks <N> --watch`.
2. Se vermelho: lê sticky comment, identifica problemas, corrige, re-roda `qa.sh` local, push.
3. **Limite de 3 iterações.** Se persistir, posta comentário pedindo intervenção humana e para.
4. Agent **nunca** declara tarefa completa com PR vermelho ou request-changes pendente.

**Forbidden** durante auto-correção:
- Subir thresholds em `clippy.toml`.
- Marcar testes como `#[ignore]` para "passar".
- Usar `--no-verify` no commit.
- Comentar lints com `#[allow(...)]` sem causa raiz justificada.
- Qualquer trial-and-error pra fazer o gate parar de reclamar.

A regra: **arrumar a causa raiz ou escalar**.

## Branch protection no `develop`

Configurar (admin do repo) com:

- Required status checks: `Quality Gate (qa.sh)` (job do `pr.yml`).
- Require branches to be up to date before merging.
- Include administrators (sem bypass).
- Dismiss stale reviews on new commits.
- Require resolution of all conversations before merging.

Sem isso, o gate pode ser ignorado.

## Calibrando thresholds

Estado inicial (issue #404):

- `QA_MIN_COVERAGE = 0` no CI — não bloqueia até calibrarmos.
- `clippy.toml` com thresholds que podem flagar código existente.

Plano de calibração:

1. Rodar `./scripts/qa.sh` localmente e registrar baseline real.
2. Ajustar `clippy.toml` para o **pior caso atual** + 0 (não regredir, mas não quebrar tudo de cara).
3. Subir `QA_MIN_COVERAGE` para o baseline atual menos margem (ex: baseline 62% → piso 60%).
4. Em issues seguintes, **apertar** thresholds gradualmente conforme dívida cai.

## Rodando rápido (cheat sheet)

```bash
./scripts/qa.sh                      # gate completo
QA_SKIP_COVERAGE=1 ./scripts/qa.sh   # rápido, sem coverage
cargo fmt --all                      # arruma formatação
cargo clippy --fix --workspace       # arruma o que clippy sabe arrumar
```

Logs por etapa: `target/qa-logs/*.log`.

## Relação com `validate.sh`

- `scripts/validate.sh` → checa **arquivos modificados** (size, fmt, clippy por crate, slint, inline tests). Útil durante edição.
- `scripts/qa.sh` → checa **workspace inteiro** (fmt + clippy + build + test + coverage). Gate de pré-push e CI.

Use `validate.sh` enquanto edita, `qa.sh` antes de pushar.
