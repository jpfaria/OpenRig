# CLI / env vars (adapter-gui)

| Argumento / Variável | Efeito |
|---|---|
| `openrig --project /path/project.openrig` | Abre o projeto direto, pula launcher (forma documentada do #436) |
| `openrig /path/project.yaml` (posicional) | Idem (forma legada, ainda aceita) |
| `OPENRIG_PROJECT_PATH=...` | Igual (env tem menor prioridade que CLI) |
| `--auto-save` ou `OPENRIG_AUTO_SAVE=1` | Salva a cada alteração, esconde botão |
| `--mcp` | Sobe servidor MCP em `http://127.0.0.1:4123` (GUI continua) — ver `docs/mcp.md` |
| `--mcp=ADDR:PORT` | Servidor MCP no endereço dado (ex.: `--mcp=0.0.0.0:9000`) |
| `--midi` | Start the MIDI/BLE-MIDI adapter using the **resolved view** (ADR 0003 / #499): project bindings (from `project.openrig`'s `midi:` block) → system fallback (`midi-bindings.yaml`) → shipped default. Controller comes from `midi-profile.yaml`. Migrates a legacy `midi-map.yaml` on first launch. See `docs/midi.md`. |
| `--midi=PATH` | Direct legacy-file load (no migration, no resolution). Useful for testing an explicit map (e.g. `--midi=~/maps/chocolate.yaml`). |
| `--render --project P --input IN.wav --output OUT.wav` | Headless offline render (issue #552). Loads `P`, pumps `IN.wav` through the chain's DSP, writes the result to `OUT.wav`. No GUI, no audio device, no MCP, no MIDI. Same `engine::offline::render_chain` used by `cargo test`. |

## Offline render mode (`--render`)

When `--render` is present, `adapter-gui` skips Slint init entirely and hands
off to `adapter-render`. The runtime behaviour is deterministic — same project
plus same input produces a byte-identical output WAV.

| Render flag | Default | Meaning |
|---|---|---|
| `--project <path>` | required | `.openrig` project file (or legacy chain YAML — migrated transparently) |
| `--input <path>` | required | Input WAV (8/16/24/32-bit PCM or 32-bit float; mono or stereo) |
| `--output <path>` | required | Output WAV path. Written atomically via `<path>.tmp` + rename — a failed render leaves no partial file |
| `--chain <id_or_description>` | first chain | Pick a specific chain from the project; matches either the chain id or its `description` field |
| `--sample-rate <Hz>` | `48000` | Output sample rate. Input WAVs are read at their native rate; the engine processes at this rate |
| `--block-size <frames>` | `256` | Internal chunk size. Should not change observable output for time-domain-stable blocks |
| `--bit-depth <16\|24\|32>` | `24` | Output sample format. `32` = 32-bit float; `16`/`24` = signed PCM |
| `--tail-ms <ms>` | `2000` | Extra silence appended after the input so reverb/delay tails are captured instead of truncated |

**Mutual exclusion** (`--render` is rejected with exit code 2 if combined with):

| Conflicting flag | Why |
|---|---|
| `--mcp` / `--mcp=ADDR:PORT` | The live MCP server requires a running rig — offline render does not bring one up |
| `--midi` / `--midi=PATH` | The MIDI adapter requires the live runtime |
| Positional project path (`openrig path.openrig --render …`) | Ambiguous which is the render target — use `--project` instead |

**Exit codes:**

| Code | Meaning |
|---|---|
| `0` | Render succeeded; `--output` written |
| `1` | Render failed (bad project file, bad input WAV, engine build error, IO error). No partial output file remains |
| `2` | Argument error (mutual exclusion, missing required flag, invalid value such as `--bit-depth 19`) |

**Scope note:** the offline driver renders a single chain from the project — no
multi-chain mixdown, no I/O block routing (the offline driver supplies the
input bus and consumes the output bus directly), no MIDI/automation replay.
This matches the analyzer pipeline's needs (issue OpenRig-claude#8); the live
rig keeps its multi-input routing untouched.

Precedência do path: `--project <PATH>` > posicional > `OPENRIG_PROJECT_PATH`
(last-wins entre formas CLI). O path resolvido é **validado** (`validate_project_path`):
não existe → `project file not found: <path>`; não é arquivo →
`project path is not a file: <path>`. Path inválido **não derruba o app** —
loga o erro e cai no launcher (alinhado com `2026-04-09-cli-project-path-autosave-design.md`;
autosave não foi reinventado). Carregar/parsear o `project.openrig` no engine
é wiring fora do escopo do #452.

Parsing em `adapter-gui/src/{cli,main,lib}.rs`. Auto-save em `sync_project_dirty()`.
