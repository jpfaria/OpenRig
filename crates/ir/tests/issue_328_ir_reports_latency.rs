//! #328: the partitioned convolver delays every sample by one partition. A
//! chain split lines its paths up from what each block reports, so a cab in
//! one path only must report exactly the delay it adds — measured here with a
//! unit impulse response, whose output is the input, one partition late.

use block_core::{MonoProcessor, StereoProcessor};
use ir::{MonoIrProcessor, StereoIrProcessor, PARTITION_SIZE};

fn first_loud(samples: impl Iterator<Item = f32>) -> usize {
    samples
        .enumerate()
        .find(|(_, s)| s.abs() > 0.5)
        .map(|(i, _)| i)
        .expect("the impulse comes out")
}

#[test]
fn mono_convolver_reports_the_delay_it_adds() {
    let mut ir = MonoIrProcessor::new(vec![1.0]).expect("unit impulse response");
    let mut buffer = vec![0.0_f32; 4 * PARTITION_SIZE];
    buffer[0] = 1.0;
    ir.process_block(&mut buffer);
    let measured = first_loud(buffer.iter().copied());
    assert_eq!(
        ir.latency_samples(),
        measured,
        "the convolver must report the {measured}-sample delay it adds"
    );
}

#[test]
fn stereo_convolver_reports_the_delay_it_adds() {
    let mut ir = StereoIrProcessor::new(vec![1.0], vec![1.0]).expect("unit impulse responses");
    let mut buffer = vec![[0.0_f32; 2]; 4 * PARTITION_SIZE];
    buffer[0] = [1.0, 1.0];
    ir.process_block(&mut buffer);
    let measured = first_loud(buffer.iter().map(|frame| frame[0]));
    assert_eq!(
        ir.latency_samples(),
        measured,
        "the stereo convolver must report the {measured}-sample delay it adds"
    );
}
