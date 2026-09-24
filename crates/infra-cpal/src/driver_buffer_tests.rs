use cpal::{BufferSize, SupportedBufferSize};

use super::{callback_capacity_frames, requested_buffer};

#[test]
fn asio_streams_take_the_drivers_buffer() {
    assert_eq!(requested_buffer(true, 64), BufferSize::Default);
}

#[test]
fn other_hosts_keep_the_projects_buffer() {
    assert_eq!(requested_buffer(false, 64), BufferSize::Fixed(64));
}

#[test]
fn asio_preallocates_up_to_the_drivers_maximum() {
    let range = SupportedBufferSize::Range { min: 32, max: 2048 };
    assert_eq!(callback_capacity_frames(true, 64, &range), 2048);
    // A driver whose maximum is below the project's request.
    let small = SupportedBufferSize::Range { min: 32, max: 32 };
    assert_eq!(callback_capacity_frames(true, 64, &small), 64);
}

#[test]
fn other_hosts_preallocate_the_requested_size() {
    let range = SupportedBufferSize::Range { min: 32, max: 2048 };
    assert_eq!(callback_capacity_frames(false, 64, &range), 64);
    assert_eq!(
        callback_capacity_frames(true, 64, &SupportedBufferSize::Unknown),
        64
    );
}
