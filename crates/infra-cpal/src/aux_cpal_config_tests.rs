use cpal::{BufferSize, SampleFormat, SupportedBufferSize, SupportedStreamConfig};

use super::aux_cpal_config;
use crate::aux_output_cpal::AUX_MAX_FRAMES;

fn supported(buffer: SupportedBufferSize) -> SupportedStreamConfig {
    SupportedStreamConfig::new(2, 48_000, buffer, SampleFormat::I32)
}

// #978: an auxiliary stream on ASIO takes the driver's buffer, like the
// chain streams, and preallocates up to the driver's maximum.
#[test]
fn an_asio_aux_stream_takes_the_drivers_buffer() {
    let range = SupportedBufferSize::Range {
        min: 32,
        max: 16_384,
    };
    let format = aux_cpal_config(true, &supported(range), 48_000, 256);

    assert_eq!(format.config.buffer_size, BufferSize::Default);
    assert_eq!(format.max_frames, 16_384);
}

#[test]
fn an_asio_driver_with_a_small_maximum_keeps_the_aux_floor() {
    let range = SupportedBufferSize::Range { min: 32, max: 512 };
    let format = aux_cpal_config(true, &supported(range), 48_000, 256);

    assert_eq!(format.max_frames, AUX_MAX_FRAMES);
}

#[test]
fn other_hosts_keep_the_aux_request() {
    let range = SupportedBufferSize::Range {
        min: 32,
        max: 16_384,
    };
    let format = aux_cpal_config(false, &supported(range), 44_100, 256);

    assert_eq!(format.config.buffer_size, BufferSize::Fixed(256));
    assert_eq!(format.config.sample_rate, 44_100);
    assert_eq!(format.config.channels, 2);
    assert_eq!(format.max_frames, AUX_MAX_FRAMES);
}
