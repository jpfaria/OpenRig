# Real-Time Latency Measurement — Design Spec

## Problem

The latency shown in the UI (`LAT 3ms`) is calculated from buffer sizes only: `(input_buffer + output_buffer) / sample_rate`. This doesn't include the ElasticBuffer (~5ms), Insert round-trip, or actual system delays.

## Solution

Measure latency in real-time using timestamps in the audio callbacks. Input callback marks when frames arrive, output callback measures when they leave. The difference is the actual system latency.

## Mechanism

### Timestamps
- `process_input_f32`: stores `Instant::now()` as nanoseconds in `AtomicU64`
- `process_output_f32`: reads input timestamp, calculates `now - input_timestamp`
- Result smoothed with exponential moving average (~100 batches)

### Shared State in ChainRuntimeState
```rust
pub struct ChainRuntimeState {
    processing: Mutex<ChainProcessingState>,
    output: Mutex<ChainOutputState>,
    // ... existing fields ...
    last_input_nanos: AtomicU64,      // timestamp of last input callback
    measured_latency_nanos: AtomicU64, // smoothed latency measurement
}
```

### Input Callback (process_input_f32)
```rust
// At the start, before processing:
let now = std::time::Instant::now().elapsed(); // or platform monotonic clock
runtime.last_input_nanos.store(now_nanos, Ordering::Relaxed);
```

### Output Callback (process_output_f32)
```rust
// After popping frames:
let input_nanos = runtime.last_input_nanos.load(Ordering::Relaxed);
if input_nanos > 0 {
    let now_nanos = ...;
    let delta = now_nanos.saturating_sub(input_nanos);
    // Exponential moving average: new = old * 0.95 + measured * 0.05
    let old = runtime.measured_latency_nanos.load(Ordering::Relaxed);
    let smoothed = if old == 0 { delta } else { (old * 95 + delta * 5) / 100 };
    runtime.measured_latency_nanos.store(smoothed, Ordering::Relaxed);
}
```

### GUI Reading
- Poll every ~500ms (existing timer)
- Read `measured_latency_nanos` from runtime, convert to ms
- Display in LAT chip with color indicator

### Color Indicator
- **Green** (#29df85): < 10ms — excellent
- **Yellow** (#f2cb54): 10-20ms — acceptable
- **Red** (#f06292): > 20ms — noticeable lag

## What This Measures
- Input device buffer latency
- Processing time (blocks)
- ElasticBuffer delay
- Output device buffer latency

## What This Does NOT Measure
- AD/DA converter latency (~1ms each)
- USB transport latency (~0.5-1ms)
- Total adds ~2-3ms not captured (same as DAWs)

## Files to Modify
- `crates/engine/src/runtime.rs` — add AtomicU64 fields, timestamp in process_input/output
- `crates/adapter-gui/src/lib.rs` — read measured latency, update LAT display
- `crates/adapter-gui/ui/pages/project_chains.slint` — color the LAT chip based on value

## Testing
- Test smoothing calculation
- Test zero latency when no input yet
- Test latency value is positive and reasonable (< 100ms)
