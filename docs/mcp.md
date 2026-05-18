# MCP server

OpenRig expõe um servidor **MCP (Model Context Protocol)** opcional. Ele **não**
é um modo que substitui a GUI: é um servidor de rede **complementar** que liga
na instância viva (GUI ou console). Você usa a GUI; um agente (Claude Desktop,
Claude Code, Cursor, …) opera a **mesma rig** pelo MCP. Os dois compartilham um
único `ProjectSession` — mudou pela GUI, o agente vê; o agente mexeu, a GUI
reflete.

## Habilitar

Flag opt-in (ausente = servidor não sobe, zero overhead):

| Forma | Efeito |
|---|---|
| `openrig --mcp` | Sobe MCP em `http://127.0.0.1:4123` (GUI continua aberta) |
| `openrig --mcp=ADDR:PORT` | Sobe no endereço dado (ex.: `--mcp=0.0.0.0:9000`) |
| `openrig --mcp=...` inválido | Loga o erro e **não** sobe (app segue normal) |

Vale igual no console: `adapter-console --mcp[=ADDR]`.

Transporte: **Streamable HTTP** (padrão atual de MCP). stdio fica como
follow-up.

## Superfície

- **Tools** — uma por variante de `Command` (schema JSON auto-derivado de
  `application::command`; zero schema escrito à mão). O agente adiciona blocos,
  muda parâmetros, troca preset, salva projeto, etc.
- **Resources** (read-only): `openrig://project` (YAML do projeto atual),
  `openrig://devices` (dispositivos de áudio).
- **Prompts**: `tune_tone`, `diagnose_chain`, `build_preset`,
  `analyze_reference`.

## Plugin OpenRig (recomendado p/ end-user)

O próprio repo é o plugin Claude (manifest na raiz: `.claude-plugin/plugin.json`
+ `.mcp.json` → `http://127.0.0.1:4123` + `skills/openrig-tone-builder/`).
Instalar o plugin = o cliente já conecta no `openrig --mcp` rodando, sem
config manual, e a skill de timbre dirige a rig pelas tools.

> `.claude/skills/` no repo é só **skills de desenvolvedor** (code-quality,
> rust/slint best-practices). Skills end-user vivem no plugin.

## Configurar um cliente (manual, sem o plugin)

`claude_desktop_config.json` (ou config MCP do Claude Code), apontando para a
instância já rodando:

```json
{
  "mcpServers": {
    "openrig": { "url": "http://127.0.0.1:4123" }
  }
}
```

1. Abra o OpenRig com `openrig --mcp` (GUI normal + servidor).
2. Adicione o bloco acima na config do cliente MCP.
3. O cliente lista as tools (uma por `Command`) e os resources; pode ler o
   estado e executar comandos que mutam a rig viva.

## Nota operacional — disputa de device

Cada instância OpenRig que sobe áudio toma o device. Se você roda **duas**
instâncias no **mesmo** device de áudio, elas disputam. Rode o agente contra a
instância que já é dona do device (a GUI/console aberta), não uma segunda
instância paralela no mesmo device.

## Arquitetura (resumo)

`crates/adapter-mcp` é biblioteca frontend-agnóstica (`rmcp` 1.7.0). O frontend
é dono do `LocalDispatcher` (`!Send`, thread do frontend); o MCP roda em thread
própria (tokio) e atravessa a fronteira por `application::bridge` (canal `Send`
+ `futures` oneshot). Drenado a cada tick na thread do frontend — mesmo caminho
dos callbacks da GUI. Zero código de audio thread tocado; invariantes 1–10
preservados por construção. Spec:
`docs/superpowers/specs/2026-05-17-165-mcp-server-design.md`.
