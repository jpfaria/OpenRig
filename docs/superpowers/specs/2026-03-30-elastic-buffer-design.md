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
