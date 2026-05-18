# Testes

## ⛔ TDD obrigatório — RED-FIRST. Proibido implementar sem teste que falha antes

**Lei do projeto, não recomendação.** É **proibido** escrever ou alterar código de
produção sem um teste que **falhou primeiro**. Teste escrito depois da
implementação (que passa de imediato) não prova nada e "vicia" a suíte — também
proibido.

**Para corrigir um bug, nesta ordem:**

1. **Entrevistar quem reportou** — cenário exato, dados, passos, resultado
   esperado vs. obtido. Não adivinhar.
2. **Escrever um teste que reproduz o bug** pelo caminho mais real possível
   (o caminho que o app executa).
3. **Rodar e ver FALHAR** (RED real). Mostrar a falha. Se o teste passa, ele
   não pegou o bug → refazer; ou, se não for bug de lógica (ex.: renderização
   Slint, que unit test não exercita), **dizer isso honestamente e parar**.
4. **Só então** investigar e corrigir, guiado pelo teste, até passar (GREEN).
5. Rodar a suíte cheia + invariantes de áudio.

**Provar que um teste não é "viciado":** reverter SÓ a produção para o estado
pré-fix (mantendo os testes) e rodar — tem que dar RED. Restaurar a produção
depois (nada se perde; está commitado).

Detalhamento e casos reais: `.claude/skills/openrig-code-quality/SKILL.md`
(seção "LEI — RED-FIRST OBRIGATÓRIO").

## Cobertura

- **Ferramenta**: `cargo-llvm-cov` (instalar com `cargo install cargo-llvm-cov` + `rustup component add llvm-tools-preview`)
- **Script local**: `scripts/coverage.sh` — gera relatório HTML em `coverage/`
- **CI**: `.github/workflows/test.yml` — informativo, sem gate

## Convenções

- `#[cfg(test)] mod tests`
- Nomes: `<behavior>_<scenario>_<expected>` (ex.: `validate_project_rejects_empty_chains`)
- Sem framework externo. Helpers no próprio módulo.

## Categorias

- **Integração com áudio real**: `#[ignore]` (rodar com `cargo test -- --ignored`)
- **DSP nativos**: golden samples com tolerância `1e-4`, processar silêncio/sine, verificar non-NaN
- **NAM/LV2/IR builds**: `#[ignore]` (assets externos)
- **Registry tests** em block-* crates: iterar TODOS os modelos via registry

## Workspace

```bash
cargo test --workspace
```

(~1100+ testes)
