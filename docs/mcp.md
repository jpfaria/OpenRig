# MCP server

OpenRig exposes an optional **MCP (Model Context Protocol)** server. It is
**not** a mode that replaces the GUI: it is a **complementary network server**
that attaches to the live instance (GUI or console). You use the GUI; an agent
(Claude Desktop, Claude Code, Cursor, …) drives the **same rig** over MCP. Both
share one `ProjectSession` — a change made in the GUI is seen by the agent, and
a change made by the agent is reflected in the GUI in real time.

## Enable the server

Two ways (#712). Persistent enablement is the per-machine `mcp_enabled`
switch in `config.yaml` (default off), toggled from **Settings → System /
Integrations → MCP server** — packaged builds, launched with no
arguments, rely on this; it binds the default `127.0.0.1:4123` and takes
effect on the next launch.

The CLI flag (absent = no config override) **forces** the server on for a
single run, and is the only way to pick a non-default address:

| Form | Effect |
|---|---|
| `openrig --mcp` | Forces MCP up at `http://127.0.0.1:4123` for this run (GUI stays open) |
| `openrig --mcp=ADDR:PORT` | Forces it up at the given address (e.g. `--mcp=0.0.0.0:9000`) |
| `openrig --mcp=...` invalid | Logs the error and does **not** start (app runs normally) |

Same flag on the console: `adapter-console --mcp[=ADDR]`.

Transport: **Streamable HTTP** (the current MCP default). stdio is a
follow-up.

## Surface

- **Tools** — one per `Command` variant (JSON schema auto-derived from
  `application::command`; no hand-written schema). The agent adds blocks,
  changes parameters, switches presets, saves the project, etc. Includes
  `render_chain` (`Command::RenderChain`, #576) — an offline render that
  applies a chain/preset YAML to a WAV and writes the processed output
  WAV via the same `adapter-render` call site as `openrig-render`. Paths
  are local to the host; live capture stays in the binary. See
  `docs/render.md`. Also includes `refresh_audio_devices`
  (`Command::RefreshAudioDevices`, #829) — re-enumerate the interfaces
  after a USB hot-swap without touching the GUI — and the Tone Doctor
  pair (#791): `diagnose_chain_tone`
  (`ToneDoctorCommand::DiagnoseChainTone` — runs the offline
  blame-by-ablation over the chain's DI, or its live input when no DI is
  loaded) and `apply_tone_doctor_fix` (`ApplyToneDoctorFix` — applies
  the measured correction from that chain's last diagnosis). The
  diagnosis is expensive and completes asynchronously: the tool call
  returns once accepted, and the verdict is read back from
  `openrig://chains/{chain}/tone`. See `docs/tone-doctor.md`.
- **Resources** (read-only):
  - `openrig://project` — current project as YAML.
  - `openrig://devices` — available audio devices.
  - `openrig://ids` — chain/block IDs (for `midi-map.yaml`).
  - `openrig://meters` — per-chain peak meters (dBFS).
  - `openrig://tuner` (#829) — live tuner readings: `running`,
    `reference_hz`, and one row per (chain, input, channel) tap with
    `note`, `octave`, `cents`, `frequency`, `active` (JSON). The rows
    are empty and `running` is `false` while the analyzer is powered
    off — dispatch `SetTunerEnabled` first.
  - `openrig://spectrum` (#829) — live spectrum readings: `running`,
    the shared `band_hz` center frequencies, and one row per tap with
    `levels` / `peaks` (0.0..1.0 per band) (JSON). Powered on with
    `SetSpectrumEnabled`.
  - `openrig://di` (#829) — per-chain DI loop state: `playing`, the
    playback `in_dbfs` / `out_dbfs`, and the loaded `source` (JSON).
  - `openrig://chains/{chain}/latency` (#829) — measured DSP latency for
    one chain, probed at that chain input's real rate and buffer (never a
    hardcoded 48 kHz), plus the `sample_rate` / `buffer_frames` used.
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
  - `openrig://chains/{chain}/quality` (#791) — objective quality
    report for one chain (THD+N, noise floor, peak/RMS level, dynamic
    range, clipping) under a `quality` envelope (JSON).
  - `openrig://chains/{chain}/tone` (#791) — the chain's last Tone
    Doctor run: `state` (`idle` / `running` / `ok` / `failed`), `error`
    (why it failed), and `tone` — the verdict: `symptom`, `severity`,
    `culprit` (block id) + `culprit_label`, the `fizz`/`mud`/`boom`/
    `clip` measurements with the limit each is judged by, and
    `suggestion` (the measured fix: block, param path, current,
    suggested, optional `enable_path`). `diagnose_chain_tone` returns
    as soon as the run is accepted, so poll this until `state` leaves
    `running`.
  - `openrig://paths` (#582) — effective resolved system paths
    (`data_root`, `presets_path`, `plugins_path`, `evaluations_path`)
    as a JSON object. Every value is an absolute path: when the user
    has not set an override in `config.yaml`, the resource returns the
    OS default a consumer would compute itself. Skills (e.g.
    `openrig-tone-analyzer`) read this instead of hard-coding
    `~/Library/Application Support/OpenRig/…`.

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

Reads follow the same contract from the other direction: every
`openrig://*` resource resolves through the one `application::read::resolve`
matcher, which reads live state through `application::live_source::LiveSource`
instead of a concrete GUI/console type. A resource has exactly **one**
payload shape no matter which adapter is mounted, because `resolve` — not
the frontend — owns the serialization. A frontend that hosts no source for a
given read (the console has no tuner/spectrum/DI runtime, for example) still
answers the documented empty shape instead of a refusal, so the resource
stays addressable on every transport. See [Architecture](architecture.md)
for the `LiveSource` hosting rule and the `devices()` / `chain_loopers()`
tri-state.

See also: [CLI & env vars](cli.md) · [Architecture](architecture.md) · design
spec `docs/superpowers/specs/2026-05-17-165-mcp-server-design.md`.
