# Gitflow — OpenRig

```
Issue → Branch (from develop) → Commits → PR → Review/Merge
```

| Branch | Propósito | Merge into |
|---|---|---|
| `main` | Releases | — |
| `develop` | Próxima release | `main` |
| `feature/*` | Funcionalidades | `develop` |
| `bugfix/*` | Correções | `develop` |
| `hotfix/*` | Urgências em produção | `main` + `develop` |
| `release/*` | Preparação de release | `main` + `develop` |

## Regras

1. **Issue primeiro.** `gh issue list --search` antes de criar (evita duplicata). NUNCA criar issue sem pedido explícito do usuário.
2. **Nome de branch: `feature/issue-{N}` ou `bugfix/issue-{N}`** — sem sufixo descritivo. Antes de criar: `git fetch && git branch -a | grep issue-{N}`.
3. **A partir de develop atualizado**: `git checkout develop && git pull`.
4. **Mergear develop antes de qualquer trabalho**: `git merge -X theirs origin/develop`.
5. Commits em inglês, sem `Co-Authored-By`, foco no "why".
6. **NUNCA `Closes #N` ou `Fixes #N`** em commits — GitHub auto-fecha.
7. Bugfix/hotfix mergeia imediato. Feature aguarda review. Nunca mergear feature→develop sem o usuário pedir.
8. **NUNCA rebase.** Sempre `git merge`, nunca `git pull --rebase`.
9. **Push imediato após cada commit.**

## Fechar issue

Só quando o usuário pedir. Antes do close, atribuir milestone:

1. `gh release list --limit 20` → última `vX.Y.Z-dev.N`.
2. Próximo milestone = `vX.Y.Z-dev.(N+1)`. Criar se não existir: `gh api repos/<owner>/<repo>/milestones -f title=... -f state=open`.
3. Sem ciclo dev → próximo milestone aberto comum (perguntar se houver dúvida).
4. `gh issue edit <N> --milestone "..."` → `gh issue close <N>`.

NUNCA atribuir ao milestone de release final puro durante ciclo dev.

## Labels que excluem das release notes

- `duplicate` — escopo idêntico a outra issue (a duplicata é a mais nova).
- `internal` — CI/CD, scripts, workflows, build deps, configs, planejamento, mudanças não visíveis ao usuário final.

## Workspace isolado (.solvers/)

NUNCA editar código no workspace principal. Cada agent trabalha numa cópia:

```bash
rsync -a --exclude='target' --exclude='.solvers' . .solvers/issue-{N}/
cd .solvers/issue-{N} && git fetch origin
# branch existe? checkout. não existe? checkout develop && pull && checkout -b feature/issue-{N}
```

Após merge+close: `rm -rf .solvers/issue-{N}/`.

## Issues irmãs

Identificação: o **corpo** começa com `> **Sibling issues (co-evoluem neste ciclo):** #<outra>`. Antes de qualquer implementação numa issue irmã: `git fetch && git merge origin/feature/issue-<irma> --no-edit && cargo build --workspace`. Sync a cada commit lógico.

## Rastreabilidade — comentários na issue

A issue é o log de auditoria. Comentar em: plano antes de começar; cada push (hash + arquivos + build/teste); mudança de plano; cada problema com evidência; análise técnica; merges; validação em hardware; resumo final. Após `git push` ou análise técnica, próximo comando é `gh issue comment <N>`. Opções A/B/C ao usuário vão na issue ANTES da pergunta.
