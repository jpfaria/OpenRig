# Testes

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
