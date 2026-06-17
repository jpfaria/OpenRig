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

```
Hardware → InputBlock { device_id, mode, channels }
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

A chain's live hardware input can be temporarily replaced by a looping dry-DI buffer for tone-shaping without playing (#614). The substitution is per-chain and audio-thread-safe: decoding, resampling, and loop crossfading happen off the audio thread; the audio thread performs only a lock-free pointer read (zero allocation, zero lock). All other chains continue reading their own hardware inputs — isolation invariant #4 is preserved. On a chain with **multiple input sources**, the loop plays exactly **once** (through the first source segment); the remaining segments are muted while the loop is armed, so playback is never doubled at the output (#699). The state is runtime-only and is never persisted (ADR 0003). See the **Virtual DI loop** entry under **Chains** in `docs/screens.md` for UI details and source options.

### Per-entry stream isolation (issues #350 / #703)

Every **raw input entry** of a chain owns its own isolated
`ChainRuntimeState` (CLAUDE.md invariant #4): its own `processing` Mutex,
`output_routes` (+ `ElasticBuffer`), `input_taps`, scratch. The
`RuntimeGraph` is keyed by `(ChainId, entry group)`; "chain" in the YAML
is only logical grouping.

- **Two devices** (#350 phase 3): one cpal stream per device, each bound
  to its own runtime; the shared output device sums them at the backend
  (the only mix point invariant #4 permits).
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

### Per-binding stream routing (issue #716)

Chain Input/Output blocks are **ports**. An input port references an io
binding (`io`) + an endpoint (`endpoint`); an output port likewise. The
binding registry (`config.yaml` `io_bindings`, type
`domain::io_binding::IoBinding`) resolves a port to its concrete device
endpoint at build time — never on the audio thread.

A **stream** is spawned for each `(input port, output port)` pair that
belongs to the **same binding**, with input position ≤ output position in
chain block order. The stream reads the input port's endpoint, runs ONLY
the effect blocks strictly between the two ports, and writes the output
port's endpoint. Because pairing is scoped to a binding, the input of
binding A can **never** reach the output of binding B — structural
isolation (CLAUDE.md invariant #4), not a runtime check.

Worked examples (chain `A,B,C,D,E`):

- io XYZ in {ch1@0, ch2@afterA}, out {ch3,4@end} → streams
  `ch1: A B C D E → ch3,4` and `ch2: B C D E → ch3,4`.
- io XYZ in {ch1@0}, out {ch3@end, ch4@afterC} → streams
  `ch1: A B C D E → ch3` and `ch1: A B C → ch4`.

Two streams that write the SAME output endpoint share one route (summed at
the route); cross-binding sums never happen because the resolver forbids
the pair. Each segment routes to ONLY its binding's output route — that
single-output routing is what blocks the cross-binding bleed the
chain-shared cartesian path produced.

Resolution lives in `crates/engine/src/io_routing.rs`
(`resolve_chain_streams`); the registry-aware graph build is
`engine::runtime::build_io_runtime_graph` (in `runtime_io_graph.rs`).
**Legacy / unbound** blocks (empty `io`, still carrying `entries`) keep the
existing `entries`-based path — byte-identical to `build_runtime_graph`.

Contract tests: `crates/engine/src/io_binding_isolation_tests.rs`
(cross-binding bleed) + `io_binding_routing_tests.rs` (pairing + block
ranges).

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

## I/O como blocos

`Input`, `Output`, `Insert` são variantes de `AudioBlockKind` dentro de `chain.blocks`. Não existem listas separadas.

- `blocks[0]` = InputBlock (fixo, não removível)
- `blocks[N-1]` = OutputBlock (fixo, não removível)
- Inputs/Outputs/Inserts extras podem ser inseridos no meio
- Cada Input cria stream paralelo isolado; Output é tap não-destrutivo
- Insert divide a chain em segmentos; desabilitado = bypass (sinal passa direto)

Exemplo YAML mínimo:

```yaml
chains:
  - description: guitar 1
    instrument: electric_guitar
    blocks:
      - { type: input,  model: standard, enabled: true, entries: [{ name: In1, device_id: "...", mode: mono, channels: [0] }] }
      - { type: preamp, model: marshall_jcm_800_2203, enabled: true, params: { volume: 70, gain: 40 } }
      - { type: insert, model: external_loop, enabled: true, send: {...}, return_: {...} }
      - { type: delay,  model: digital_clean, enabled: true, params: { time_ms: 350, feedback: 40, mix: 30 } }
      - { type: output, model: standard, enabled: true, entries: [{ name: Out1, device_id: "...", mode: stereo, channels: [0,1] }] }
```

Sample rates: 44.1/48/88.2/96 kHz. Buffer sizes: 32/64/128/256/512/1024. Bit depths: 16/24/32. YAML antigo (`inputs:`/`outputs:` separados, `input_device_id`/`output_device_id` únicos) é migrado automaticamente.

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
