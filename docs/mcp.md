# MCP server

OpenRig exposes an optional **MCP (Model Context Protocol)** server. It is
**not** a mode that replaces the GUI: it is a **complementary network server**
that attaches to the live instance (GUI or console). You use the GUI; an agent
(Claude Desktop, Claude Code, Cursor, …) drives the **same rig** over MCP. Both
share one `ProjectSession` — a change made in the GUI is seen by the agent, and
a change made by the agent is reflected in the GUI in real time.

## Enable the server

Opt-in flag (absent = server does not start, zero overhead):

| Form | Effect |
|---|---|
| `openrig --mcp` | Starts MCP at `http://127.0.0.1:4123` (GUI stays open) |
| `openrig --mcp=ADDR:PORT` | Starts at the given address (e.g. `--mcp=0.0.0.0:9000`) |
| `openrig --mcp=...` invalid | Logs the error and does **not** start (app runs normally) |

Same flag on the console: `adapter-console --mcp[=ADDR]`.

Transport: **Streamable HTTP** (the current MCP default). stdio is a
follow-up.

## Surface

- **Tools** — one per `Command` variant (JSON schema auto-derived from
  `application::command`; no hand-written schema). The agent adds blocks,
  changes parameters, switches presets, saves the project, etc.
- **Resources** (read-only):
  - `openrig://project` — current project as YAML.
  - `openrig://devices` — available audio devices.
  - `openrig://ids` — chain/block IDs (for `midi-map.yaml`).
  - `openrig://meters` — per-chain peak meters (dBFS).
  - `openrig://presets` — project preset pool (JSON).
  - `openrig://chains/{chain}/presets` — chain preset bank (JSON).
  - `openrig://plugins` — full plugin catalog (JSON).
  - `openrig://plugins/{id}` — single plugin entry by manifest id (JSON).
  - `openrig://plugins/search/{query}` — case-insensitive substring
    search across `id` / `display_name` / `brand` (JSON).
  - `openrig://plugins/{id}/params` — catalog-level parameter schema
    for one plugin: kind, range, options, default, unit, widget (JSON).
    Unknown id → `{"params": null}`.
  - `openrig://chains/{chain}/blocks/{block}/params` — placed-block
    parameter snapshot: schema **plus** `current_value` per parameter
    (JSON, wrapped under a `params` envelope). Unknown chain / block
    → error from the bridge.

  All reads return JSON unless the type is documented as YAML or
  newline-delimited text.
- **Prompts**: `tune_tone`, `diagnose_chain`, `build_preset`,
  `analyze_reference`.

## Install the OpenRig plugin (recommended)

The end-user Claude Code plugin lives in a dedicated repository:
**[jpfaria/OpenRig-claude](https://github.com/jpfaria/OpenRig-claude)**.

Layout there:

```
.claude-plugin/plugin.json        # plugin manifest
.claude-plugin/marketplace.json   # marketplace entry (source ".")
.mcp.json                         # declares the MCP server (http://127.0.0.1:4123)
skills/openrig-tone-builder/      # end-user skill, bundled with the plugin
```

Installing the plugin auto-wires the MCP server (via `.mcp.json`) and ships
the `openrig-tone-builder` skill — no manual client config.

### Claude Code

```
/plugin marketplace add jpfaria/OpenRig-claude
/plugin install openrig@openrig
```

Then start OpenRig with the server on: `openrig --mcp`. The plugin's
`.mcp.json` points the client at `http://127.0.0.1:4123`; the client lists one
tool per `Command`, the `openrig://*` resources listed in the
[Surface](#surface) section, and the prompts. The `openrig-tone-builder` skill activates when you ask for an
artist/song tone and drives the rig through the tools.

### Claude Desktop

Settings → **Connectors** → Add custom connector → URL
`http://127.0.0.1:4123` (HTTP). Start OpenRig with `openrig --mcp` first.
(The classic `command`-based config entry is stdio-only, which v1 does not
use.)

> `.claude/skills/` in this repo holds **developer** skills only
> (`openrig-code-quality`, `rust-best-practices`, `slint-best-practices`).
> End-user skills live in the
> [OpenRig-claude](https://github.com/jpfaria/OpenRig-claude) plugin.

## Configure a client manually (without the plugin)

Point any MCP client at the running instance:

```json
{
  "mcpServers": {
    "openrig": { "url": "http://127.0.0.1:4123" }
  }
}
```

1. Start OpenRig with `openrig --mcp` (normal GUI + server).
2. Add the entry above to the MCP client config.
3. The client lists the tools (one per `Command`) and the resources; it can
   read state and run commands that mutate the live rig.

## Operational note — device contention

Every OpenRig instance that starts audio takes the device. Running **two**
instances on the **same** audio device contends. Point the agent at the
instance that already owns the device (the open GUI/console), not a second
parallel instance on the same device.

## Architecture (summary)

`crates/adapter-mcp` is a frontend-agnostic library (`rmcp` 1.7.0). The
frontend owns the `LocalDispatcher` (`!Send`, on the frontend thread); the MCP
server runs on its own tokio thread and crosses the boundary through
`application::bridge` (a `Send` channel + `futures` oneshot). It is drained
each tick on the frontend thread — the same path GUI callbacks use. No
audio-thread code is touched; invariants 1–10 hold by construction.

See also: [CLI & env vars](cli.md) · [Architecture](architecture.md) · design
spec `docs/superpowers/specs/2026-05-17-165-mcp-server-design.md`.
