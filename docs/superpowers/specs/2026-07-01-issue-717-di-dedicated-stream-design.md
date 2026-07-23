# Issue #717 — DI loop as a dedicated, isolated stream (design)

> Status: DRAFT for owner review. Supersedes the narrower "output routing"
> framing in the original #717 body — the owner clarified the DI must be a
> **completely separate stream**, not the guitar's stream routed elsewhere.

## Problem

Today an armed DI loop **replaces the guitar's input** at the chain's first
input segment (#614): the loop is fed into the SAME per-input runtime, through
the SAME block graph, out the SAME output. The owner sees the DI riding the
guitar's input meters ("ele ta usando o mesmo stream da minha guitarra… o
gráfico entrega") and it bleeds into the main guitar output.

The owner wants the DI to be a **completely separate stream**, processed
through the chain's blocks (amp/cab/pedals), on an output they choose — so it
never touches the guitar's stream, meters, or main output.

## Goal

When a chain's DI loop is armed:

1. It plays on its **own isolated runtime** — never injected into the guitar's
   input runtime. The guitar's input meters and live signal are unaffected.
2. It is processed through a **copy of the chain's block graph** (amp/cab/
   pedals), so it sounds like it went through the rig.
3. It is routed to a **per-chain chosen output** (persisted in the project),
   selected from a second select in the DI panel.
4. The guitar keeps playing live on its own stream simultaneously (a natural
   consequence of true isolation — the guitar runtime is untouched).

## Non-goals

- Backing-track / multi-track playback, tempo sync, transport. The DI is a
  single looping mono/stereo buffer, as today.
- Changing the guitar signal path in any way.
- Persisting the DI *source* choice (still ephemeral, per #614/#324). Only the
  **output routing** is persisted (it travels with the chain — ADR 0003).

## Approach: dedicated parallel DI runtime

Reuse the isolation model (invariant #4: every stream is a fully isolated
runtime; mixing happens in the backend, never in our code). Arming the DI
builds a **second runtime for the chain** whose:

- **input** = the DI loop buffer (`DiLoop`/`DiPcm`), not a hardware device;
- **blocks** = a fresh **copy** of the chain's block graph (independent DSP
  state — sharing instances would violate isolation and cause artifacts);
- **output** = the chosen output endpoint's route, at that output's rate
  (reusing the #749 per-output-rate resample).

Disarming tears this runtime down. The guitar runtime(s) are never touched, so
guitar + DI coexist, fully isolated. This removes the DI from the guitar's
input segment entirely (undoing the #614 input-substitution for this path).

### The crux — how an input-less stream is clocked (OPEN, first plan task)

A normal runtime is driven by its **input device callback** (`process_input_f32`
is called by cpal at the device rate). A DI-only runtime has **no input
device**, so nothing calls it. Its driver must come from elsewhere. Two
candidate mechanisms — the first plan task is a spike to pick one by reading
`crates/infra-cpal/src/stream_builder.rs` + `controller.rs`:

- **(a) Output-driven generator.** The chosen output device's stream drives the
  DI: its callback pulls the loop → runs the block copy → writes into that
  output's frames. Keeps everything on one real clock (the output device), but
  puts DSP in the output callback and couples the DI path to the output stream
  lifecycle.
- **(b) Input-less runtime on a companion input/virtual clock.** The DI runtime
  keeps the normal input→SPSC→output shape, but its "input" is a virtual source
  clocked at the output rate (a dummy/loopback input stream, or the output
  device opened in duplex). Preserves the existing input→SPSC→output separation
  but needs a clock source that doesn't drift against the output device.

**Decision gate:** whichever mechanism keeps the audio-thread invariants (zero
alloc/lock/syscall/IO on the callback; no xruns; isolation) with the least new
surface wins. This is documented, not assumed — no code until the spike settles
it and a red test pins the chosen behavior.

## Data model — persisted per-chain DI output (ADR 0003)

New per-chain field: the chosen DI **output endpoint** — one of the chain's
**already-bound output endpoints** (owner decision: the select lists the outputs
this chain already uses, NOT every system device). It travels with the chain, so
it lives **inside the chain in `project.openrig`** (project config), not
`config.yaml`. Absent → default to the chain's **main output** (current
behaviour, so existing projects are unchanged).

Exact field shape (an endpoint reference vs. an index into the chain's output
bindings) is settled in the plan against `crates/project/src/chain.rs` +
`crates/domain/src/io_binding.rs`.

## Command / Query parity (architecture law)

- New **`Command`** (state-changing) to set the chain's DI output — e.g.
  `SetChainDiLoopOutput { chain, output }` — with an `Event`, a
  `LocalDispatcher` handler, and GUI dispatch. MCP/gRPC inherit it (parity
  test stays green).
- If the GUI reads the chosen output back, it goes through a **`Query`** too
  (read parity).
- The dedicated DI-stream graph reads the **DI runtime's own meters** (input/
  output peaks). Those are a window the GUI has, so per the read-parity law they
  are served from `QueryKind` (MCP/gRPC see the DI stream too), not a GUI-only
  path.

## UI

**A. Output select.** Second select in the existing `DiLoopPanel` (the reusable
select component), listing the chain's **already-bound output endpoints**;
picking one dispatches the new Command. Panel layout stays the owner-approved
shape (fone → panel with source select + play/stop), with the output select
added.

**B. Dedicated DI-stream graph (owner requirement).** While the DI is armed, the
screen shows a **second signal-flow graph specific to the DI stream** — the same
visual as the guitar chain graph (IN → blocks → OUT with input/output meters),
but for the DI runtime: **IN = the DI source**, the **block copy**, **OUT = the
chosen output**, with the DI stream's **own meters**. It appears only while the
DI plays and disappears on stop. This makes the isolation visible: two graphs,
two independent meter sets — the DI is manifestly not on the guitar's stream.
Exact placement/layout (below the chain graph vs. a stacked panel) is decided in
the UI phase with `ui-ux-pro-max` + `slint` + a headless render, not assumed
here.

i18n: any new `@tr` keys added to the `.pot` + all locale `.po` in the same
commit.

## Invariants / risks

- **Isolation (#4):** the DI runtime shares no buffer/route/tap/DSP state with
  the guitar runtime. The block copy is independent.
- **CPU:** while the DI plays, the chain's DSP runs twice (guitar + DI copy).
  Acceptable for a transient monitoring feature; must not xrun (verified on the
  real-hardware battery, `OPENRIG_HW_TESTS=1`).
- **Latency/quality:** the DI path reuses the #749 per-output-rate resample; no
  change to the guitar path, so guitar latency/quality cannot regress.

## Testing (red-first)

1. **DI does not ride the guitar runtime.** Arming the DI leaves the guitar
   input runtime's segments fed by the device (its input tap/meter unaffected);
   the loop is NOT on the guitar runtime. (RED against today's input-injection.)
2. **DI plays on its own runtime at the chosen output's rate** (reuses the
   #749 length-per-rate assertion, now on the dedicated DI runtime).
3. **Chosen output persists** — round-trips through `project.openrig`.
4. **Command parity** — the new variant is covered; MCP tool count matches.
5. **Real-hardware:** no xruns with guitar + DI running together.

## Resolved decisions (owner)

1. **Processing:** the DI stream runs through a **copy of the chain's blocks**
   (amp/cab/pedals), not dry.
2. **Isolation:** the DI is a **dedicated parallel runtime**; the guitar runtime
   is untouched (guitar + DI coexist).
3. **Output select:** lists the chain's **already-bound output endpoints**
   (not every system device).
4. **Default output:** the chain's **main output** when none is chosen
   (existing projects unchanged).

## Still to settle in the plan (not owner-facing)

- The DI-stream **clock/driver** mechanism (spike: candidates (a)/(b) above).
- Exact persisted field shape in `chain.rs` (endpoint ref vs. binding index).
