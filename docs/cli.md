# CLI / env vars (adapter-gui)

| Argumento / Variável | Efeito |
|---|---|
| `openrig --project /path/project.openrig` | Abre o projeto direto, pula launcher (forma documentada do #436) |
| `openrig /path/project.yaml` (posicional) | Idem (forma legada, ainda aceita) |
| `OPENRIG_PROJECT_PATH=...` | Igual (env tem menor prioridade que CLI) |
| `--auto-save` ou `OPENRIG_AUTO_SAVE=1` | Salva a cada alteração, esconde botão |
| `--mcp` | Sobe servidor MCP em `http://127.0.0.1:4123` (GUI continua) — ver `docs/mcp.md` |
| `--mcp=ADDR:PORT` | Servidor MCP no endereço dado (ex.: `--mcp=0.0.0.0:9000`) |
| `--midi` | Sobe o adapter MIDI/BLE-MIDI com o `midi-map.yaml` padrão por-OS (GUI continua) — ver `docs/midi.md` |
| `--midi=PATH` | Idem com mapa explícito (ex.: `--midi=~/maps/chocolate.yaml`) |

Precedência do path: `--project <PATH>` > posicional > `OPENRIG_PROJECT_PATH`
(last-wins entre formas CLI). O path resolvido é **validado** (`validate_project_path`):
não existe → `project file not found: <path>`; não é arquivo →
`project path is not a file: <path>`. Path inválido **não derruba o app** —
loga o erro e cai no launcher (alinhado com `2026-04-09-cli-project-path-autosave-design.md`;
autosave não foi reinventado). Carregar/parsear o `project.openrig` no engine
é wiring fora do escopo do #452.

Parsing em `adapter-gui/src/{cli,main,lib}.rs`. Auto-save em `sync_project_dirty()`.
