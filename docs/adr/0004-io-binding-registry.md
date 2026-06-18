# ADR 0004: I/O Binding Registry

## Status

Accepted

## Context

Before this ADR, audio device endpoints (device id, channels, mode) were embedded
directly inside chain blocks (`entries: [{ name, device_id, mode, channels }]`).
That produced two structural problems:

1. **Intra-chain bleed.** Every input stream wrote to the chain-shared
   `output_routes` buffer, so with two inputs and two outputs in one chain every
   input reached every output (all-to-all). There was no way to express
   "audio from interface A exits only through interface A".
2. **Project portability.** A raw `device_id` is machine-specific. Moving an
   `.openrig` to another machine left the chain referencing a device id that may
   not exist there.

ADR 0003 established the portability test: *"if I send this `.openrig` to another
machine, does this value have to travel with it?"* Raw device ids fail that test;
they belong in `config.yaml` (system scope).

ADR 0002 established device identity via `device_id` and a conflict rule based
on physical input ownership. ADR 0004 extends that routing model by introducing a
named indirection layer rather than revising ADR 0002's identity or conflict rules.
ADR 0002 is **superseded** for the routing section (chains no longer embed
`input_device_id` / `output_device_id` directly; the registry resolves them) but
its device-identity and conflict rules remain in force.

## Decision

### Registry location and schema

A per-machine **I/O binding registry** lives in `config.yaml` under the
`io_bindings` key (system scope, per ADR 0003). Each entry groups a set of named
input endpoints and a set of named output endpoints. The endpoint struct is the
existing `{ name, device_id, mode, channels }`; it moves from the chain block
into the registry.

```yaml
io_bindings:
  - id: main           # stable id referenced by chains
    name: "Scarlett"
    inputs:
      - { name: In1, device_id: "coreaudio:...", mode: mono, channels: [0] }
    outputs:
      - { name: Out1, device_id: "coreaudio:...", mode: stereo, channels: [0,1] }
```

**Scope rationale.** The registry references concrete `device_id` / channels,
which are machine-specific (ADR 0003 system criterion). Chains reference a
binding by its stable `id`. Moving a `.openrig` to another machine carries only
the `id` reference; the target machine re-resolves it against its local registry.
This makes projects *more* portable than the legacy model where raw `device_id`
was embedded in the chain.

### Chain blocks become ports

Input/Output blocks stop carrying device endpoints. Each block becomes a *port*
that carries `io: <binding-id>` and `endpoint: <endpoint-name>`:

```yaml
- { type: input,  io: main, endpoint: In1, enabled: true }
- { type: output, io: main, endpoint: Out1, enabled: true }
```

### Routing rule

A **stream** is spawned for each `(input port, output port)` pair that belongs to
the **same binding**, with the input port at or before the output port in block
order. The stream reads the input port's source endpoint, runs only the effect
blocks strictly between the two ports, and writes the output port's destination
endpoint.

Because pairing is scoped to a binding, the input of binding A can never reach
the output of binding B — structural isolation (CLAUDE.md invariant #4), not a
runtime check.

Worked examples (chain blocks A, B, C, D, E; single binding XYZ):

```
# Input port in the middle (ch2 inserted after A):
ch1 → A B C D E → ch3,4       (head-input × tail-output)
ch2 →   B C D E → ch3,4       (middle-input × tail-output)

# Output port in the middle (ch4 tapped after C):
ch1 → A B C D E → ch3         (head-input × tail-output)
ch1 → A B C     → ch4         (head-input × middle-output)
```

### Inserts stay raw (scope decision)

Insert blocks keep their raw send/return endpoints and are **not** migrated to
the registry. An insert is a single-runtime send/return pipeline, not a
binding-paired stream; forcing it through the registry would add complexity
without benefit and risks a regression in the insert path. This is a deliberate
scope decision for issue #716.

### Commands (system scope)

New command variants in `crates/application/src/command.rs`:

- `CreateIoBinding { id, name, inputs, outputs }`
- `UpdateIoBinding { id, name?, inputs?, outputs? }`
- `DeleteIoBinding { id }` — rejected if any chain references the binding

Reshaped chain-IO commands (`SaveChainInputEndpoints`,
`SaveChainOutputEndpoints`, `SaveChainIo`) operate on `{ io, endpoint }`
references instead of embedded endpoints. MCP tooling inherits the same variants
(command-bus parity, CLAUDE.md Law 1).

### Migration

Old chains with embedded `entries` are auto-migrated on project load:

1. Collect all input and output entries from the chain.
2. Create one generated binding holding all of them (identical bindings across
   chains are merged).
3. Rewrite chain blocks to `{ io, endpoint }` references.

One binding with N inputs × M outputs pairs all-inputs × all-outputs, reproducing
the old all-to-all behavior. Golden samples and volume invariants stay
byte-identical after migration.

Old `config.yaml` without `io_bindings` and old project YAML with embedded
`entries` keep deserializing (back-compat per the YAML versioning ADR).

## Consequences

- **Cross-binding bleed is structurally impossible.** A→A / B→B routing is the
  natural, default expression; cross-device routing cannot be authored.
- **Portable projects.** The `.openrig` carries only stable binding ids; raw
  device ids live on the machine that owns them.
- **Migration is silent and sound-preserving.** Legacy projects auto-migrate
  with no user action and no audible change.
- **Future settings have a written home.** Per ADR 0003, any new per-machine
  device reference belongs in `config.yaml` alongside the registry.
- **Inserts are a known gap.** If a future issue adds insert-to-registry
  migration, it is additive and does not conflict with this ADR.

## Relations

- **Extends ADR 0002** (device identity, conflict rule). ADR 0002's
  `input_device_id` / `output_device_id` routing fields are superseded; its
  device-identity (`device_id`) and conflict rules (one physical input channel
  per enabled chain) remain valid and are now enforced at binding resolution time.
- **Applies ADR 0003** portability test: registry → system; chain port refs →
  project.
- **Relates to ADR 0001** (project runtime model). The chain block list shape
  is unchanged; only the content of Input/Output block entries changes.
