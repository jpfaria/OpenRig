//! Issue #698 — diagnostic probe: ask the default input device for a
//! 64-frame buffer at 44.1 kHz (the owner's config.yaml setting) and print
//! the buffer size CoreAudio ACTUALLY delivers to the callback. The owner's
//! worker log shows a 2902 us period (= 128 frames @ 44.1 kHz) while the
//! config requests 64 (1451 us) — this pins which side is lying.
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use hw_harness::hw_tests_enabled;

#[test]
fn granted_input_buffer_matches_requested() {
    if !hw_tests_enabled("granted_input_buffer_matches_requested") {
        return;
    }
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = host.default_input_device().expect("default input device");
    eprintln!("[#698 PROBE] device: {:?}", device.id());
    let supported = device
        .supported_input_configs()
        .expect("supported configs")
        .next()
        .expect("at least one config");
    let channels = supported.channels();

    let config = cpal::StreamConfig {
        channels,
        sample_rate: 44_100,
        buffer_size: cpal::BufferSize::Fixed(64),
    };
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<usize>::new()));
    let seen_cb = std::sync::Arc::clone(&seen);
    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _: &_| {
                let mut s = seen_cb.lock().unwrap();
                if s.len() < 50 {
                    s.push(data.len() / channels as usize);
                }
            },
            |err| eprintln!("[#698 PROBE] stream error: {err}"),
            None,
        )
        .expect("build input stream at 64/44.1k");
    stream.play().expect("play");
    std::thread::sleep(std::time::Duration::from_secs(2));
    drop(stream);

    let frames = seen.lock().unwrap().clone();
    let mut histogram = std::collections::BTreeMap::new();
    for f in &frames {
        *histogram.entry(*f).or_insert(0u32) += 1;
    }
    eprintln!(
        "[#698 PROBE] requested 64 frames @ 44.1 kHz, {} channels — granted \
         callback sizes (frames x count): {histogram:?}",
        channels
    );
    assert!(
        !frames.is_empty(),
        "no input callbacks fired in 2 s — device did not start"
    );
    assert!(
        frames.iter().all(|&f| f == 64),
        "BUG #698: requested a 64-frame buffer but CoreAudio delivers \
         {histogram:?} — the worker period (and the UI buffer setting) do \
         not match the real device cycle."
    );
}
