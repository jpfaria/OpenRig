# Elastic Buffer for Clock Drift — Design Spec

## Problem

Audio devices with independent clocks (e.g., Scarlett input + MacBook output, or Insert Send/Return via MK-300) cause artifacts: clicks, pops, dropouts. The output device consumes frames at a slightly different rate than the input produces, leading to buffer underruns (silence) or overruns (latency buildup + sudden drop).

## Solution

Replace the `VecDeque<AudioFrame>` in each `OutputRoutingState` with an `ElasticBuffer` that maintains a target queue level (~256 frames / ~5ms @ 44.1kHz). The buffer absorbs clock drift by:
- **Underrun**: repeating the last frame instead of returning silence
- **Overrun**: discarding the oldest frame (1 at a time, not in bulk)

## Component: ElasticBuffer

```rust
pub struct ElasticBuffer {
    queue: VecDeque<AudioFrame>,
    target_level: usize,        // 256 frames (~5ms @ 44.1kHz/48kHz)
    last_frame: AudioFrame,     // last frame seen, used for underrun repeat
}

impl ElasticBuffer {
    pub fn new(target_level: usize, layout: AudioChannelLayout) -> Self {
        Self {
            queue: VecDeque::with_capacity(target_level * 2),
            target_level,
            last_frame: silent_frame(layout),
        }
    }

    /// Push a frame from the input thread.
    /// If queue exceeds 2x target, discard oldest frame.
    pub fn push(&mut self, frame: AudioFrame) {
        self.last_frame = frame;
        self.queue.push_back(frame);
        if self.queue.len() > self.target_level * 2 {
            self.queue.pop_front(); // discard oldest, 1 at a time
        }
    }

    /// Pop a frame for the output thread.
    /// If queue is empty, repeat last frame instead of silence.
    pub fn pop(&mut self) -> AudioFrame {
        self.queue.pop_front().unwrap_or(self.last_frame)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }
}
```

## Target Level

- **256 frames** at any sample rate
- At 44.1kHz: ~5.8ms latency
- At 48kHz: ~5.3ms latency
- Acceptable for live monitoring — imperceptible to musicians

## Overrun Threshold

- When queue exceeds `target_level * 2` (512 frames), discard 1 oldest frame per push
- This is gradual — no sudden bulk discard, no audible artifact
- Latency naturally stabilizes around target_level

## Underrun Behavior

- When queue is empty, return `last_frame` (repeat)
- Repeating 1 frame is imperceptible in musical audio
- Much better than current behavior (silence → click)

## Files to Modify

### `crates/engine/src/runtime.rs`
- Add `ElasticBuffer` struct
- Replace `queue: VecDeque<AudioFrame>` in `OutputRoutingState` with `buffer: ElasticBuffer`
- Remove `trim_output_queue()` function
- Remove `MAX_BUFFERED_OUTPUT_FRAMES` constant
- `process_input_f32`: use `buffer.push(frame)` instead of `queue.push_back(frame)` + mix logic
- `process_output_f32`: use `buffer.pop()` instead of `queue.pop_front().unwrap_or(silent_frame())`

### Nothing else changes
- CPAL callbacks unchanged
- GUI unchanged
- YAML unchanged
- Frame format unchanged
- try_lock behavior unchanged

## Future: CoreAudio Aggregate Device (macOS)

The `ElasticBuffer` is the cross-platform solution. On macOS, a future optimization can use CoreAudio's Aggregate Device API to synchronize clocks at the OS level, eliminating drift entirely with zero added latency.

To prepare for this:
- The `ElasticBuffer` replaces the buffer in `OutputRoutingState` directly (no trait indirection for now — YAGNI)
- When CoreAudio is implemented, the macOS path can bypass the elastic buffer or use a thin wrapper
- The engine doesn't need to know which mechanism is active

## Testing

- Test push/pop basic behavior
- Test underrun: pop from empty buffer returns last_frame, not silence
- Test overrun: pushing beyond 2x target discards oldest
- Test queue stabilizes around target_level under simulated drift
- Test mix_frames still works with ElasticBuffer

## Success Criteria

- No clicks/pops when using Scarlett input + MacBook speaker output
- No clicks/pops when using Insert Send/Return via MK-300
- No clicks/pops when using multiple outputs on different devices
- Latency increase <= 6ms
- Same device input+output: no behavior change (buffer stays near empty)

## Addendum — IR cold-start cushion (issue #592)

The buffer "stays near empty" assumption above holds for light chains, but
a convolution (IR/cab) block runs a full FFT **inline** once per
`ir::PARTITION_SIZE` (512) samples. At small device buffers (32/64) that
periodic spike is far heavier than the other callbacks, so on a **cold**
first stream start — before the DSP producer warms up — the near-empty
buffer drains to silence on the spike and the output crackles/distorts
until a warm rebuild. (This is what made a freshly-loaded IR preset sound
distorted until the user nudged a knob.)

Fix (`engine::elastic_prime`): for a chain that contains an enabled
convolution block, the output `ElasticBuffer` is

1. **sized** to hold at least one convolver partition
   (`elastic_capacity_target` floors the device-derived target at
   `ir::PARTITION_SIZE`), and
2. **primed** with that many silent frames on the **initial build only**
   (`ElasticBuffer::prime`, gated by `existing_blocks.is_none()`), so the
   cushion exists from frame 0.

A rebuild/edit is **not** primed — the producer is already warm, so it
refills naturally; re-priming each knob turn would add a silence gap.
Non-convolution chains are untouched (no extra latency). The cushion costs
one partition (~10.7 ms @ 48 kHz) of added output latency on IR chains at
small buffers — the deliberate "more buffer when using IR" trade, ranked
below stream stability in the trade-off hierarchy.

## Addendum — spike eliminated at the source (issue #617)

The #592 cushion above treated the symptom: it gave the output buffer
enough slack to ride out the convolver's periodic per-partition FFT burst.
But the burst itself — running the whole partition's FFT + frequency-delay-
line multiply-accumulate **inline** every `ir::PARTITION_SIZE` samples —
still overran the *steady-state* 64-frame budget on tight/slow setups (the
cushion only helps the cold start; once primed it is consumed and a
recurring spike larger than the per-callback budget still underruns).

#617 removes the burst at the source by shrinking `ir::PARTITION_SIZE` from
512 to **64** (≤ the smallest supported device buffer). Each 64-frame
callback now does exactly one partition's worth of work — the cost is
**uniform across callbacks, with no spike** — and the convolver's added
latency drops from ~10.7 ms to ~1.3 ms. The average per-callback cost rises
(up to 128 partitions for an 8192-sample IR) but stays tiny in absolute
terms; per the trade-off hierarchy (stability > CPU) this is the right call.

Consequently the cold-start cushion is now **decoupled** from the partition
size: `engine::elastic_prime::IR_COLD_START_CUSHION_FRAMES` (512) replaces
the `ir::PARTITION_SIZE` floor, kept at the proven #592 magnitude solely to
absorb generic first-callback producer warmup jitter — no longer the
(now-eliminated) spike.
