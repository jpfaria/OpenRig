# CLI / env vars (adapter-gui)

| Argumento / Variável | Efeito |
|---|---|
| `openrig /path/project.yaml` (posicional) | Abre direto, pula launcher |
| `OPENRIG_PROJECT_PATH=...` | Igual ao posicional (env tem menor prioridade) |
| `--auto-save` ou `OPENRIG_AUTO_SAVE=1` | Salva a cada alteração, esconde botão |

Parsing em `adapter-gui/src/{main,lib}.rs`. Auto-save em `sync_project_dirty()`.
