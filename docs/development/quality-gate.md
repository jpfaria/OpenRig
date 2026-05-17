# Quality Gate — OpenRig

OpenRig **não tem mais gate interno**. Usa o gate **compartilhado** mantido
centralmente em [`github.com/xgodev/quality-gate`](https://github.com/xgodev/quality-gate),
igual para todos os projetos. Mesma filosofia de sempre: **comparativo**,
falha só quando o PR **piora** uma métrica vs `develop`; dívida preexistente
nunca bloqueia.

> Migração: issue #482 (removeu `scripts/qa.sh` + gate interno).
> Canônico de uso/contrato: `~/.quality-gate/docs/` após clonar.

## TL;DR

```bash
# primeira vez (clona) — depois, manter atualizado:
git -C ~/.quality-gate pull --ff-only \
  || git clone --depth 1 https://github.com/xgodev/quality-gate.git ~/.quality-gate

~/.quality-gate/qg --base origin/develop
```

Vermelho → arrumar a **causa raiz** do que regrediu → rodar de novo → só
então `git push`.

### Agentes (Claude Code)

A skill **`claude-plugin:quality-gate`** faz isso automaticamente. Triggers:
"rodar quality gate", "rodar QG", "verificar qualidade", "validar antes do
PR", "qa antes do push" (e equivalentes em EN). Ela clona/atualiza
`~/.quality-gate`, roda o dispatcher e interpreta o JSON. Instalação:

```
/plugin marketplace add git@github.com:xgodev/claude-plugin.git
/plugin install claude-plugin
```

## Filosofia (inalterada)

O gate quebra quando o PR **piora** o projeto. Não importa quanta dívida
preexistente exista — se o PR não a aumenta, passa. Cada PR pode reduzir
dívida; nunca aumentar.

## Métricas comparadas (PR vs base) — Rust

| Métrica | Como conta |
|---|---|
| `fmt` | `cargo fmt --check` (rulesets embutidos do gate) |
| `lint` | `cargo clippy -D warnings` sem complexity |
| `build` | `cargo build --all-targets` |
| `test` | `cargo test --no-fail-fast` (soma de falhas) |
| `complexity` | clippy cognitive/lines/args/type (defaults do gate) |
| `coverage` | `cargo llvm-cov` → `lines.percent`, margem `QG_COV_MARGIN` |

> O gate **ignora** `clippy.toml`/`rustfmt.toml` do projeto de propósito
> (tamper-resistance) e usa rulesets próprios (defaults da comunidade:
> cognitive 25, lines 100, args 7, type 250). Nosso `clippy.toml` continua
> valendo só pro `cargo clippy` local e pro `scripts/validate.sh`.

## Local vs CI — mesmo dispatcher

| Aspecto | Local | CI (`.github/workflows/pr.yml`) |
|---|---|---|
| Comando | `~/.quality-gate/qg --base origin/develop` | mesmo `qg`, clonado no job |
| Baseline | `git archive origin/develop` (cache em `/tmp`) | `--baseline-dir baseline/` (checkout paralelo) + `--force-full` |
| Falha no CI | — | sticky comment (header `openrig-quality-gate`) + `request-changes` formal do `github-actions[bot]`; sucesso → comment ✅ + dismissal |
| Codecov | — | reusa profraw do PR → `lcov.info` → upload |

## Exit codes

| Código | Significado |
|---|---|
| 0 | Passou / bypass / sem linguagem suportada relevante |
| 1 | Regrediu ≥1 métrica vs base |
| 2 | Erro de ferramenta/setup (NÃO é regressão — relatar stderr) |
| 3 | Nenhuma linguagem suportada detectada |

## Env vars (prefixo `QG_`)

| Variável | Default | Uso |
|---|---|---|
| `QG_BASE_REF` | (vazio) | = `--base`. Vazio → modo absoluto |
| `QG_BASELINE_DIR` | (vazio) | = `--baseline-dir` (checkout pronto, CI) |
| `QG_COV_MARGIN` | `1.0` | Tolerância (pp) de coverage |
| `QG_LOG_DIR` | `target/qg-logs` | Logs por etapa |
| `QG_FORCE_FULL` | `0` | `1` = desliga fast-path |
| `QG_FORMAT` | `text` | `text` ou `json` |
| `QG_BYPASS_REASON` | (vazio) | **NUNCA setar por conta própria.** Força exit 0 + audit log |

## Validação de negócio — testes obrigatórios

Cobertura sozinha não basta. Toda lógica nova ou alterada **exige teste que
valide comportamento esperado**, não apenas execute o caminho.

- Bug fix → teste vermelho que reproduz o bug primeiro, fix depois (TDD).
- Feature → teste cobrindo cenário esperado **e** edge cases.
- Refactor que muda comportamento observável → teste antes do refactor.

## Forbidden

Pra silenciar o gate sem fix real:

- `QG_BYPASS_REASON` por iniciativa própria.
- Subir thresholds em `clippy.toml` (não adianta — o gate ignora) ou editar
  código/teste/config só pra "passar".
- Marcar testes como `#[ignore]`.
- `#[allow(clippy::...)]` sem causa raiz justificada.
- `--no-verify` no commit.

A regra: **causa raiz ou escalar**.

## `validate.sh` — checagem estática por-arquivo (continua)

`scripts/validate.sh` **não é o gate** e segue existindo: caps de LOC,
`cargo fmt`/`clippy` por-arquivo, compile Slint, proibição de
`#[cfg(test)] mod tests` inline. É a operacionalização das regras OpenRig
que o gate genérico não cobre. Rodar a cada arquivo `.rs`/`.slint` tocado.
