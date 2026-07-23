# Configuração de áudio

## Stream model (CLAUDE.md invariantes 4 / 5 / 10)

### Regras do stream

1. **Bus interno é SEMPRE estéreo.** Mono input vira `Stereo([s, s])`
   logo no começo via `to_stereo` (broadcast). Não há momento dentro
   do chain em que o sinal trafega como mono no bus.
2. **Cada bloco declara um `ModelAudioMode`** que indica o layout que
   sabe processar (`MonoOnly` / `DualMono` / `TrueStereo` /
   `MonoToStereo`).
3. **O wrapper do bloco só insere conversão se o bloco exige outro
   layout.** Bus já em estéreo + bloco que aceita estéreo → sem
   conversão.
4. **Output mode `mono`** é o único caso que volta a colapsar pra
   1 canal, via `mixdown(L, R)`. Stereo output passa direto.

### Tabela de wrappers

Bus interno é SEMPRE estéreo. Input `dual_mono` ou `stereo` já entram como
estéreo (sem conversão). Input `mono` faz `to_stereo` (broadcast L=R=s)
no início e o bus segue estéreo daí pra frente. Cada bloco pede o
layout que sabe processar, e o wrapper só insere conversão se o bus
estéreo não bate com o que o bloco aceita.

| `ModelAudioMode` | Wrapper antes | Wrapper depois | Comportamento do bloco |
|---|---|---|---|
| `MonoOnly` | `to_mono` (mixdown L+R) | `to_stereo` (broadcast) | 1 instância processa o sample colapsado |
| `DualMono` | `to_dual_mono` (marca 2 mono indep) | `to_stereo` (marca par estéreo) | 2 instâncias paralelas, uma por canal — `[L,R]` indep entram, `[L_p, R_p]` saem |
| `TrueStereo` | passa direto (bus já estéreo) | passa direto | 1 instância vê `[L, R]` correlacionados, processa stereo |
| `MonoToStereo` | (recebe estéreo, talvez `to_mono` se a impl exige source mono) | passa direto | bloco devolve estéreo |

Pinned em `crates/engine/src/runtime_block_builders.rs` ~L444-497.

### Pipeline canônico

> **Model A (#716):** an `InputBlock`/`OutputBlock` is a `{ model, io, endpoint }`
> reference to an I/O binding — it carries **no** device/channels. At activation
> `engine::runtime_endpoints::resolve_chain_io(chain, registry)` resolves each
> port to its concrete device endpoint (`device_id`/`mode`/`channels`) from the
> per-machine binding registry (`config.yaml` `io_bindings`). The pipeline below
> shows the resolved endpoint feeding the (unchanged) stereo bus.

```
Hardware → [InputBlock io/endpoint] --resolve_chain_io--> { device_id, mode, channels }
        ↓
  bus inicial:
    mode mono     → Mono → to_stereo (broadcast L=R=s) → Stereo
    mode dual_mono → Stereo (semântica: 2 mono indep)
    mode stereo    → Stereo (semântica: par estéreo)
        ↓
  pra cada bloco no chain:
    [wrapper antes]   → adapta bus pro layout que o bloco aceita
    bloco processa
    [wrapper depois]  → devolve pro bus estéreo
        ↓
  OutputBlock { device_id, mode, channels }
    mode stereo, ch [a, b]  → ch_a = L, ch_b = R
    mode mono,   ch [a]     → mixdown(L, R) → s, escreve ch_a
    mode mono,   ch [a, b…] → mixdown(L, R) → s, replicado em TODOS

  Mixdown:
    Average → (L + R) * 0.5  (default)
    Sum     →  L + R
    Left    →  L
    Right   →  R
```

> **Device opens at its NATIVE channel count** (issue #516). When `mode = mono`
> with `channels = [a]` selects a single physical output on a hardware-stereo
> interface (Scarlett 2i2 etc.), the CPAL stream is still opened at the
> device's `default_output_config().channels()`. Opening such a device as
> 1-channel mono silences it on macOS / CoreAudio — the routing is the
> engine's job (`write_output_frame` writes the mixdown into `ch_a` of the
> interleaved buffer; other channels stay at zero).

### Exemplos

**1. Mono in + bloco MonoOnly + stereo out**
```
HW Mono(ch0) → to_stereo → [s,s] → to_mono → block_mono(s)→m → to_stereo
            → [m,m] → HW(ch0=m, ch1=m)
```

**2. Stereo in + bloco DualMono + stereo out**
```
HW Stereo(ch0,ch1) → [L,R] → to_dual_mono → block_dm(2 instâncias):
       L→L_p, R→R_p → to_stereo → [L_p,R_p] → HW(ch0=L_p, ch1=R_p)
```

**3. Mono in + bloco TrueStereo (chorus) + stereo out**
```
HW Mono(ch0) → to_stereo → [s,s] → block_ts(L=R=s)→[L',R']
            → HW(ch0=L', ch1=R')
```

**4. Mono in + MonoOnly + TrueStereo + stereo out**
```
HW Mono(ch0) → to_stereo → [s,s] → to_mono → block_mono→m → to_stereo
            → [m,m] → block_ts→[L',R'] → HW(ch0=L', ch1=R')
```

**5. Mono in + bloco MonoOnly + mono out**
```
HW Mono(ch0) → to_stereo → [s,s] → to_mono → block_mono→m → to_stereo
            → [m,m] → mixdown=m → HW(ch0=m)
```

### Streams paralelos

- Cada InputBlock = um stream paralelo TOTALMENTE isolado (próprio
  runtime, sem buffer / lock / route / tap compartilhado).
- Múltiplos InputBlocks / OutputBlocks → soma é responsabilidade do
  backend (cpal / JACK). O engine NUNCA mistura streams entre si.
- Solo input passa unity em qualquer combinação (Input mode × Output
  mode), pinned em `crates/engine/src/volume_invariants_tests.rs`.

### Por que essas regras (invariantes 4 / 5 / 10)

- **#4 — Isolation entre streams.** Cada InputBlock tem o próprio
  runtime, próprio buffer, próprio estado. Mexer num não afeta outro.
  Mistura final é trabalho do driver de áudio.
- **#5 — Bus estéreo internamente.** Mono input vira Stereo([s, s])
  desde o primeiro bloco. Blocos sempre veem `[L, R]`. Decisão de
  como sair (mono / stereo) é só do OutputBlock.
- **#10 — Volume por stream IMUTÁVEL.** Nada no engine atenua o
  signal preemptivamente "pra evitar clip". Output limiter (`tanh`
  no fim) cuida disso. Solo input passa unity em qualquer combinação
  de Input mode × Output mode (pinned por `volume_invariants_tests.rs`).

### Split-mono fan-out

Caso especial: `mode: mono` com **mais de um canal** em `channels`
(`channels: [0, 1]`). O engine cria **um sibling stream por canal** —
cada um roda a chain inteira em paralelo, lendo um canal físico
diferente. Útil pra duas guitarras na mesma interface usando o mesmo
preset, sem precisar duplicar a chain.

Pra **uma única fonte mono** (1 guitarra), use `channels: [N]` apenas
(N = canal físico onde a fonte entra). Múltiplos canais ativam o
fan-out e provavelmente não é o que você quer.

Acceptance pinned (`volume_invariants_tests` g01..g04):

- `solo` (signal só em ch0, ch1 silencioso) → output peak = signal peak (UNITY).
- `dual` abaixo do limiter knee (ch0 + ch1 com signal) → soma direta.
- `dual` acima do knee → `tanh(soma)`.
- `mono → stereo bus broadcast` é simétrico (L = R).

`split_mono_sibling_count` é metadata estrutural; o multiplier de scale
**MUST stay at 1.0** até feature opt-in de auto-mix existir com aprovação
explícita do usuário (`crates/engine/src/runtime.rs` ~L334-339).

### Virtual DI loop (per-chain, ephemeral)

A chain's DI loop plays on its **own isolated, streamed runtime** (#771) — it never replaces or rides the guitar's live input. Arming resolves the chain's persisted output choice (`Chain.di_output` → one of its bound output endpoints; absent → the main output), builds a fresh copy of the chain's block graph REDUCED to that output's binding (so the loop feeds the route the chosen output drains, #716/#699) on a `di-stream` worker, and parks a ring-backed playback on that output's cell immediately — a 75 s loop starts in milliseconds (the full pre-render tried first took minutes before the first sample). The worker steps the runtime paced by **ring backpressure**: it only produces what the output callback consumed, so the output device clock IS the DI clock — no drift by construction (the sleep-paced worker tried in #717 drifted and was reverted) — and the callback only pops frames and sums (zero allocation/locks/DSP, invariant #8). Decoding, resampling (per-output rate, #749) and all block DSP happen off the audio thread. The guitar runtime, its meters, and every other output are untouched (isolation invariant #4); a device-rate change re-arms at the new rate (#669). A live edit (a param change or a block toggle) re-renders the DI **gaplessly** (#785): the playback that is sounding keeps playing while the new render is built and pre-rolled off-thread, and the incoming worker takes the output's cell over mid-loop — at exactly the loop position the listener reaches (`DiPlayback::play_pos`, `set_di_loop_pos`) — so the edit lands with neither a silent gap nor a restart of the take. The outgoing playback is retired off the audio thread, never dropped by the callback (invariant #8). The **source** choice stays runtime-only; only the **output** choice persists, inside the chain in `project.openrig` (ADR 0003). See the **Virtual DI loop** entry under **Chains** in `docs/screens.md` for UI details.

### Per-chain looper (#323)

Each `ChainRuntimeState` owns a `LooperBank` — up to 8 loopers, each up to 60 s of stereo at that runtime's **live** sample rate. The bank sits inside `ChainProcessingState` (the audio thread already holds `&mut` to it) and is driven by a lock-free op queue, the same pattern as the block-toggle fast path (#580): the control thread never takes the `processing` lock, it pushes a `LooperOp` and the audio thread drains it inside the section it already owns.

**Where it sits in the signal path.** The loopers run at the chain input, on the chain's FIRST segment only (#699: a chain's loop material is heard exactly once, no matter how many segments share the callback). They record the dry frame that feeds the segment — after the DI substitution, so recording works while monitoring a DI — and SUM their playback into it, unlike the DI loop which REPLACES the frame. The loop therefore runs through the entire block graph and follows every live edit.

**Memory and the RT contract.** Layer buffers are allocated by the control thread and handed to the audio thread inside the op; buffers a looper is done with (cleared, undone past the ring, refused) go back through a return queue and are dropped on the GUI tick. The audio thread never allocates, locks or frees (invariant #8) — pinned by `looper_record_overdub_and_undo_do_not_allocate` in `audio_alloc_invariant_tests`. Playback sums the audible layers on read (one multiply-add per layer per channel per frame), which is what makes undo/redo O(1): they move a counter instead of re-mixing 60-second buffers off-thread.

**Isolation.** A bank belongs to exactly ONE runtime. A chain served by several parallel runtimes (#703) gets one bank per runtime, each recording its own input with its own buffers — two audio threads never touch the same memory, and a chain-level status reads whichever runtime actually holds material. An off-thread rebuild carries the banks over (`adopt_taps_from`), so a live edit does not wipe a recorded loop; a rebuild that CHANGED the sample rate drops them instead of replaying frames at the wrong speed (the #669 failure mode).

### Per-entry stream isolation (issues #350 / #703)

Every **raw input entry** of a chain owns its own isolated
`ChainRuntimeState` (CLAUDE.md invariant #4): its own `processing` Mutex,
`output_routes` (+ `ElasticBuffer`), `input_taps`, scratch. The
`RuntimeGraph` is keyed by `(ChainId, entry group)`; "chain" in the YAML
is only logical grouping.

- **Two devices** (#350 phase 3): one cpal stream per device, each bound
  to its own runtime; the shared output device sums them at the backend
  (the only mix point invariant #4 permits). On macOS each device's
  callback joins **its own** device's OS workgroup — resolved by the
  bound device's UID, never the system default (#760). Before this the
  join was hard-coded to the default device, so the non-default
  interface's callback co-scheduled with the wrong device's IO thread and
  underran under CPU contention despite spare cores. The `dsp_worker`
  thread (which we own, unlike the C-owned cpal HAL callback thread) holds
  its membership in an RAII guard and **leaves** the workgroup before the
  thread exits: a chain rebuild tears the worker down and respawns it, and
  a thread that joined but exits without leaving crashes in libpthread's
  `_os_workgroup_tsd_cleanup` (#779). The HAL callback thread cannot leave
  from another thread, so it keeps its membership for the process lifetime.
- **Two entries on ONE device** (#703): Core Audio cannot open two
  streams on one device (a previous attempt produced total silence), so
  the device keeps ONE cpal stream whose callback fans out to every
  per-entry runtime bound to that cpal index. On macOS each entry gets
  its own `dsp_worker` realtime thread, so a heavy entry cannot starve
  its sibling. State isolation is the contract; the hardware deadline of
  one device callback is inherently shared.
- **Split-mono siblings** (one entry, `mode: mono, channels: [a, b]`)
  stay in ONE runtime: the pinned volume invariants (g02/g03) require
  siblings to sum before the per-runtime limiter.
- **Insert chains** are a single runtime (the send/return pipeline spans
  cpal indices); **Linux/JACK** keeps the per-device grouping behind the
  `jack` cfg because the JACK-direct client binds one runtime.

Contract tests: `crates/engine/src/stream_isolation_tests.rs` +
`stream_isolation_same_device_tests.rs`; cpal binding in
`crates/infra-cpal/src/tests_regression.rs`.

**Live edits on a VST3 chain (#779).** A live edit on a running chain normally
rebuilds its runtime off-thread and swaps it in — but a **fresh** build calls
the VST3 `createInstance` on the control worker while the audio thread is inside
the old instance's `process()`, and JUCE global state is not safe against that
concurrent instantiate-vs-process (SIGSEGV; the pairing #778's lock cannot
cover, since `process()` is RT and must not lock). So a chain containing a VST3
is instead updated **in place** (`engine::runtime::update_chain_runtime_state`
in `controller_offthread_live_rebuild.rs`): the live VST3 instance is reused (a
param change becomes `setParameter`, never a reload), mutated under the runtime's
processing lock. Non-VST3 chains keep the off-thread fresh rebuild — re-creating
a NAM/native block touches no shared JUCE state.

### I/O resolution from the binding registry (issue #716, model A)

Device I/O is **never** stored in the chain/preset/scene/rig — it lives only
in the per-machine binding registry (`config.yaml` `io_bindings`, type
`domain::io_binding::IoBinding { id, name, inputs, outputs }` of
`IoEndpoint { name, device_id, mode, channels }`). A chain references bindings
via `Chain.io_binding_ids` (its start/end I/O, never persisted as blocks) plus
optional **mid** `Input`/`Output` blocks that each carry `{ io, endpoint }`.

At activation `engine::runtime_endpoints::resolve_chain_io(chain, registry)`
turns the chain + registry into the resolved input/output endpoints
(`device_id`/`mode`/`channels`) and feeds them to the **proven, unchanged**
engine build (`build_per_input_runtime_states` / `build_runtime_graph`, which
take a `registry: &[IoBinding]`). The engine still builds one isolated runtime
per input (invariant #4) and sums at the backend per physical output endpoint;
only the **source** of the device endpoints moved (binding, not block
`entries` — which are removed). Resolution happens off the audio thread.

**Input-conflict rule (activation).** Two or more ACTIVE inputs may not share
the same `(device, channel)` — within a chain AND globally across active
chains; same device on different channels is fine; outputs may be shared (many
inputs may feed one output). `input_port_conflict` / `input_conflicting_chains`
(`runtime_endpoints.rs`) detect it; `ProjectRuntimeController::sync_project`
refuses to activate a conflicting chain (first wins). The rig path enforces the
same via `tap_conflict` (`rig_runtime.rs`).

Contract tests: `crates/engine/tests/issue_716_input_conflict.rs`
(conflict detector + skip decision); `crates/project/tests/issue_716_chain_io_bindings.rs`
+ `issue_716_binding_discovery.rs` (`resolve_chain_ports`); golden +
`volume_invariants` + `stream_isolation` prove the resolved path is bit-exact
to the legacy entries path.

### DSP worker per input stream (issue #670, macOS)

The chain DSP does NOT run inside the CoreAudio input callback. The HAL
thread sleeps between cycles; heavy model working sets (NAM A2 weights)
cool down, and the cold-cache inference tail (~1.4 ms vs ~250 us hot)
sporadically crossed the 64-frame cycle — CoreAudio then drops input,
heard as a click. Measured and fixed via
`crates/infra-cpal/tests/issue_670_real_streams_no_xruns.rs` (real
streams, real chain, DI-loop injection, 60 s): inline DSP = 12 xruns;
worker = 0 xruns / 0 underruns.

The input callback only copies the buffer into a lock-free SPSC ring
(microseconds, invariant #8 clean) and a dedicated per-stream worker
(`crates/infra-cpal/src/dsp_worker.rs`) runs `process_input_buffer`:
preemptible realtime, computation budget sized to the real work, short
spin (a bounded ~35% of the period — it keeps the model weights hot
through the inter-buffer gap, killing the cold tail) then 100 us sleeps
when idle.

What the xrun LED means under the worker: a late worker buffer that
catches up is absorbed by the ring + elastic and is NOT audible — it
feeds the load meter only. Audible damage is counted where it physically
happens: an elastic underrun (output starved) or a ring-overflow drop (a
gap in the played signal, counted as an xrun). In the old inline design a
late callback WAS damage (CoreAudio dropped input), hence the old
semantics, which the non-F32 inline paths keep. F32 input path only (the
macOS live path); the Linux/JACK backend is untouched.

### Chain enabled é runtime, não persistência

`Chain.enabled` é estado de memória — o usuário liga / desliga uma
chain enquanto o app roda. **NÃO É serializado no `project.yaml`** —
chains carregam sempre como desabilitadas e o usuário decide quais
ativar. `ChainYaml.enabled` tem `skip_serializing` por isso.

Um channel de um device físico só pode estar habilitado em **uma**
chain por vez. O runtime valida isso em memória ao habilitar.

## I/O e bindings (model A, #716)

A chain's **start/end I/O comes from the binding registry**, selected via
`Chain.io_binding_ids`, and is **never persisted as blocks**. `chain.blocks`
holds only **effects** + optional, manually-inserted **mid** `Input` / `Output`
/ `Insert` blocks. `Input`/`Output`/`Insert` are variants of `AudioBlockKind`;
there are no separate I/O lists.

- The chain's main input/output are materialized from `io_binding_ids` at
  activation (head inputs at offset 0, tail outputs at the end) — not stored.
- Mid `Input`/`Output` blocks are `{ model, io, endpoint }` ports referencing a
  binding endpoint; they carry **no** device data (legacy `entries` removed).
- Each input still spawns its own isolated parallel runtime; Output is a
  non-destructive tap; Insert splits the chain into segments (disabled = bypass).

### I/O binding registry (#716)

The binding holds the concrete device endpoint (device id, mode, channels); the
chain (and preset/scene/rig) carry only stable binding `id` references. This
keeps `.openrig` files portable — moving them to another machine re-resolves the
ids against that machine's local `config.yaml` registry.

`config.yaml` schema (system scope):

```yaml
io_bindings:
  - id: main                # stable id referenced by chains
    name: "Scarlett"
    inputs:
      - { name: In1, device_id: "coreaudio:...", mode: mono, channels: [0] }
    outputs:
      - { name: Out1, device_id: "coreaudio:...", mode: stereo, channels: [0,1] }
  - id: cab_b
    name: "Interface B"
    inputs:
      - { name: In1, device_id: "B", mode: mono, channels: [0] }
    outputs:
      - { name: Out1, device_id: "B", mode: stereo, channels: [0,1] }
```

Chain block YAML using ports:

```yaml
chains:
  - description: guitar 1
    instrument: electric_guitar
    blocks:
      - { type: input,  io: main, endpoint: In1, enabled: true }
      - { type: preamp, model: marshall_jcm_800_2203, enabled: true, params: { volume: 70, gain: 40 } }
      - { type: insert, model: external_loop, enabled: true, send: {...}, return_: {...} }
      - { type: delay,  model: digital_clean, enabled: true, params: { time_ms: 350, feedback: 40, mix: 30 } }
      - { type: output, io: main, endpoint: Out1, enabled: true }
```

Insert blocks are **not** migrated to the registry — they keep raw send/return
endpoints because an insert is a single-runtime send/return pipeline, not a
binding-paired stream. See ADR 0004.

### Legacy projects open UNBOUND (clean break, #716)

There is **no device migration**. The block model no longer has an `entries`
field at all — `io`/`endpoint` are the only I/O fields, and they are required,
so a project's device routing is never inferred from old per-block device data.
A legacy chain (or a chain whose bindings are not configured on this machine)
opens **unbound**: it produces no runtime and plays no audio until the user
selects its I/O bindings in Settings → I/O. Loading still works: the
`project.openrig` migration keeps the effect/preset/scene structure and simply
drops the old device endpoints (the user re-selects bindings).

This is intentional: routing is binding-only, the registry (`config.yaml`) is
the single source of truth for I/O, and a project remains portable without
inventing device routing the user never confirmed on this machine.

Sample rates: 44.1/48/88.2/96 kHz. Buffer sizes: 32/64/128/256/512/1024. Bit depths: 16/24/32.

## Per-machine device settings (config.yaml)

To change audio device settings in the UI, open the **Settings screen** (top bar) and select the **System / Audio interface** section. Sample rate, buffer size, bit depth, and language are **per-machine**, not per-project. They persist to `config.yaml` (#287):

- macOS: `~/Library/Application Support/OpenRig/config.yaml`
- Windows: `%APPDATA%\OpenRig\config.yaml`
- Linux: `~/.config/OpenRig/config.yaml`

Schema:

```yaml
recent_projects: [...]
paths: { thumbnails, screenshots, metadata }
input_devices: [{ device_id, name, sample_rate, buffer_size_frames, bit_depth, ... }]
output_devices: [...]
language: pt-BR  # ou en-US, ou null para seguir o OS
```

`gui-settings.yaml` legado é migrado automaticamente para `config.yaml` no primeiro boot e removido — sem ação manual.

`load_project_session()` popula `project.device_settings` em memória. YAML do projeto **não persiste** `device_settings` (`skip_serializing`), mas YAML antigo com o campo ainda deserializa.

## JACK lifecycle (Linux only)

Com feature `jack`, OpenRig controla o ciclo de vida do JACK. `ensure_jack_running()` em infra-cpal detecta a placa USB, lê SR/buffer do `device_settings`, **põe o mixer de playback da placa em unity** (`LiveJackBackend::set_playback_mixer_unity` → `amixer -c $CARD sset <ctrl> 100% unmute` nos controles comuns; best-effort, requer `alsa-utils`) — sem PipeWire/Pulse nada inicializa o mixer e muitas interfaces USB sobem atenuadas (~−23 dB → som fraco/abafado). Depois lança `jackd -d alsa -d hw:$CARD -r $SR -p $BUF -n 3`, espera o socket aparecer em `/dev/shm/`. Timer de 2s no adapter-gui (`health_timer`) verifica `is_healthy()` e tenta reconectar quando JACK volta. Tudo atrás de `#[cfg(all(target_os = "linux", feature = "jack"))]`.
