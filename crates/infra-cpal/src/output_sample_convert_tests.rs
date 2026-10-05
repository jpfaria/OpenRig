use super::{f32_to_i32, NativeOutputBuffer};

fn ramp(out: &mut [f32]) {
    for (i, s) in out.iter_mut().enumerate() {
        *s = i as f32 / 16.0 - 0.5;
    }
}

// #978: ASIO opens only the driver's native format, commonly Int32, so an
// auxiliary render (metronome, player, drums) has to reach an integer buffer
// with the same scaling the chain outputs use.
#[test]
fn an_f32_render_reaches_an_int32_buffer() {
    let mut buffer = NativeOutputBuffer::with_capacity(16);
    let mut out = [0i32; 16];

    buffer.fill(&mut out, f32_to_i32, &mut ramp);

    let mut reference = [0.0f32; 16];
    ramp(&mut reference);
    let expected: Vec<i32> = reference.iter().map(|s| f32_to_i32(*s)).collect();
    assert_eq!(out.to_vec(), expected);
}

#[test]
fn a_buffer_larger_than_the_preallocation_gets_silence_past_it() {
    let mut buffer = NativeOutputBuffer::with_capacity(8);
    let mut out = [7i32; 12];

    buffer.fill(&mut out, f32_to_i32, &mut |o: &mut [f32]| o.fill(1.0));

    assert!(out[..8].iter().all(|s| *s == i32::MAX));
    assert!(out[8..].iter().all(|s| *s == 0));
}

#[test]
fn the_render_starts_from_silence_every_cycle() {
    let mut buffer = NativeOutputBuffer::with_capacity(4);
    let mut out = [0i32; 4];
    buffer.fill(&mut out, f32_to_i32, &mut |o: &mut [f32]| o.fill(1.0));

    // A render that only mixes into what it gets must not hear last cycle.
    buffer.fill(&mut out, f32_to_i32, &mut |_: &mut [f32]| {});

    assert!(out.iter().all(|s| *s == 0));
}
