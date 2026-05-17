# MCP Server — Design (#165)

**Date:** 2026-05-17
**Issue:** #165
**Branch:** `feature/issue-165` (from `feature/issue-436`)
**Status:** Approved design — pending implementation plan

Supersedes the MCP framing in `2026-04-23-command-dispatch-architecture-design.md` Phase 5
on two points: transport is **Streamable HTTP** for v1 (not stdio); the model is a
**complementary network server attached to a live frontend instance**, not a `--mcp` launch
mode that replaces the GUI.

---

## Goal

Add an MCP (Model Context Protocol) server so any MCP-capable AI client (Claude Desktop,
Claude Code, Cursor, custom agents) can drive the **same running OpenRig instance** the user
has open. The GUI is what the user operates directly; MCP is how an agent operates the same
rig on the user's behalf. Both mutate one shared `ProjectSession` through the existing
command bus, so a change made in either is immediately seen by the other.

## Non-goals (v1)

- stdio transport (follow-up).
- Remote mode — a thin MCP proxy holding a `RemoteDispatcher` over gRPC #42 (follow-up).
- Auth beyond what local-network transport implies.
- HTTP+SSE legacy transport. Streamable HTTP only.
- AI model invocation from inside OpenRig (the model runs in the client).
- Undo/redo, command batching.

---

## Context (what already exists)

The command bus (Phase 1, #295) is **done** on `feature/issue-436`:

- `crates/application/src/command.rs` — `Command` enum, 28 variants, derives
  `Serialize + Deserialize + JsonSchema`.
- `crates/application/src/event.rs` — `Event` enum, 25 variants, same derives.
- `crates/application/src/dispatcher.rs` — `trait CommandDispatcher { fn dispatch(&self, cmd: Command) -> Result<Vec<Event>>; fn subscribe(&self) -> EventStream; }`.
- `crates/application/src/local_dispatcher.rs` — `LocalDispatcher` holding
  `Rc<RefCell<ApplicationSession>>` (single-threaded, `!Send`).
- `crates/adapter-server` — placeholder for the future gRPC adapter (#42).
- `crates/adapter-gui`, `crates/adapter-console` — frontends that own the dispatcher + engine.

`EventStream` is currently the `()` placeholder; Phase 1 left fan-out for later. This issue
introduces the first real consumer of an event stream, so it also defines the minimal
event fan-out mechanism (see *Thread bridge*).

---

## Architecture

### Principle

A **frontend** (GUI or console) owns the application: it constructs the `LocalDispatcher`,
runs the audio engine, and runs its own event loop. **MCP and gRPC are not frontends and
not launch modes** — they are optional network servers that *attach* to a running frontend
and feed/observe the same command bus. The same is true for the future gRPC adapter; this
design establishes the pattern.

```
            ┌─────────────── frontend process (GUI or console) ───────────────┐
 user ────▶ │  GUI / console event loop  ──▶ LocalDispatcher ──▶ ProjectSession │
            │        ▲   │                         ▲    │             │         │
            │        │   │ drain (per tick)        │    │ dispatch()  │ Events  │
            │   bridge│   ▼                         │    ▼             ▼         │
            │   ┌─────┴───────────────────────────────────────────────────┐    │
 agent ───▶ │   │  adapter-mcp  (own tokio thread, rmcp Streamable HTTP)   │◀───┼── Claude
            │   └──────────────────────────────────────────────────────────┘    │
            └──────────────────────────────────────────────────────────────────┘
```

### Crate

New **library** crate `crates/adapter-mcp` (mirrors `adapter-server`). Frontend-agnostic:
it knows nothing about Slint, the console, or the engine. It receives only the bridge
handles (a command sender + an event receiver, both `Send`) and the server config.

### SDK

`rmcp` 1.7.0 — the official Rust MCP SDK (`modelcontextprotocol/rust-sdk`,
Apache-2.0). Provides the server, Streamable HTTP transport, and the
tools/resources/prompts protocol surface. Tool input schemas are derived from the existing
`#[derive(JsonSchema)]` on `Command` — **no hand-written JSON Schema**.

### Transport

**Streamable HTTP**, configurable bind address/port. This lets an already-running OpenRig
(GUI or console) accept an agent connection on the live instance — the scenario the user
requires ("GUI open and also an MCP server"). stdio (client spawns a dedicated process) is
a deliberate follow-up; the bridge design does not preclude it.

### Thread bridge (the critical part)

`LocalDispatcher` is `!Send` (`Rc<RefCell<ApplicationSession>>`) and must be called on the
frontend thread. `rmcp` is async/tokio and runs on its own thread. They are connected by a
`Send` bridge owned by the frontend and handed to `adapter-mcp`:

- **Command path**: an MCP tool handler builds a `Command`, sends
  `(Command, oneshot::Sender<Result<Vec<Event>>>)` over a **bounded** `mpsc` channel. The
  frontend drains this channel on each event-loop tick (the same place GUI callbacks run
  today), calls `dispatcher.dispatch(cmd)` on the frontend thread, and replies on the
  `oneshot`. The MCP handler `await`s the reply and returns the events to the client.
- **Event path**: the frontend publishes every `Vec<Event>` produced by *any* dispatch
  (GUI-originated or MCP-originated) onto a `tokio::sync::broadcast` channel. `adapter-mcp`
  subscribes and surfaces events as MCP notifications. This is the minimal real
  `EventStream` the Phase 1 placeholder deferred.
- **Backpressure / safety**: the channel is bounded; drain is non-blocking and O(pending)
  per tick with a per-tick cap so a flood of agent calls cannot stall the frontend loop.
  No locks, no blocking, no allocation on the audio thread — **the audio thread is never
  touched by this design**. MCP only injects `Command`s into the existing bus exactly as
  the GUI does; real-time invariants 1–10 hold by construction.

### Frontend wiring

An opt-in flag/config (e.g. `--mcp[=addr:port]`, plus the equivalent in the project/app
config) makes the frontend (a) construct the bridge, (b) spawn the `adapter-mcp` server
thread, (c) drain the command channel each tick, (d) publish events to the broadcast. When
the flag is absent, nothing changes and there is zero overhead. `adapter-gui` and
`adapter-console` both gain this wiring; the wiring itself is small and shared in spirit
with the future gRPC adapter.

---

## MCP surface

### Tools — one per `Command` variant (28)

Each `Command` variant becomes one tool. The tool's input schema is the variant's
`schemars`-derived JSON Schema. The handler deserializes arguments into the `Command`,
sends it over the bridge, and returns the resulting `Vec<Event>` (serialized) as the tool
result. A test locks the generated tool set to the `Command` enum shape so adding/removing
a variant without updating nothing-extra is caught (regression guard).

Tool naming: derive from the variant name in a stable snake_case form
(e.g. `SetBlockParameterNumber` → `set_block_parameter_number`).

### Resources (read-only)

| URI | Content |
|---|---|
| `openrig://project` | current project YAML |
| `openrig://chain/{idx}` | single chain state |
| `openrig://block/{chain}/{block}` | single block + parameters |
| `openrig://devices` | available audio devices |
| `openrig://models/{block_kind}` | models for a block kind, with metadata for selection |
| `openrig://catalog/plugins` | full plugin catalog (#162) |

Resources are served by reading current `ProjectSession`/registry state through a read-only
query path over the same bridge (a `Query` request variant, or a read closure executed on
the frontend thread — chosen during planning; must not duplicate domain logic).

### Prompts

- `tune_tone` — given a target tone description, suggest parameter changes.
- `diagnose_chain` — walk the current chain and report issues.
- `build_preset` — build a preset matching a description (delegates to #163 when shipped).
- `analyze_reference` — analyze an audio file and propose a chain (delegates to #164).

Prompts in v1 are static templates that orient the agent to use the tools/resources; they
do not embed business logic.

### Notifications

The dispatcher event stream is surfaced as MCP notifications in v1 (it comes for free over
the bridge's broadcast channel). The agent sees GUI-originated changes too.

---

## Testing

- Integration: start the server against a stub frontend (no Slint), list tools, invoke a
  representative subset across the `Command` families, assert state changed via the
  matching resource.
- Schema-lock test: assert the generated tool set ⇔ `Command` variants (regression guard).
- Bridge test: command in → dispatch on the owning thread → events out, including the
  event-broadcast path, with no `Send` of the dispatcher.
- No GUI/Slint in any test (invariant: the screen has no business logic).
- Zero warnings; `./scripts/qa.sh` green before any push.

## Real-time / invariants

No DSP, routing, I/O, or audio-thread code is added or modified. MCP is purely another
`Command` producer on the existing bus. The bridge's frontend-side drain is bounded and
non-blocking and runs on the frontend thread, not the audio thread. Invariants 1–10 are
preserved by construction; the design introduces no new `Mutex`/`RwLock`/log/I/O on the
processing path.

## Documentation (same commit as code)

- `docs/` page: enabling the server, Claude Desktop config snippet, Claude Code MCP config,
  example agent flow, and the operational note that two OpenRig instances sharing one audio
  device will contend (run the agent against the instance that owns the device).
- Update the architecture doc cross-reference to point Phase 5 at this spec.

## Risks

- **rmcp API churn** — pin the version; the bridge isolates `rmcp` to one crate so a future
  bump is localized.
- **Per-tick drain cost** — capped per tick; benchmarked to confirm no added jitter to the
  frontend loop (it does not run on the audio thread, so it cannot cause xruns, but it must
  not stall the UI either).
- **Resource read path** — must reuse domain/query code, not re-derive project structure in
  the adapter (zero-coupling rule). Resolved in the implementation plan.

## Open items for the implementation plan

- Exact frontend event-loop hook for the drain in `adapter-gui` (Slint timer/idle) and
  `adapter-console`.
- Read-only query mechanism for resources (request variant vs. read closure).
- Tool-name derivation helper location (in `application` next to `Command`, or in
  `adapter-mcp`).
