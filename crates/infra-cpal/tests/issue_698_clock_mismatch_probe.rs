//! Issue #698 — probe: the CoreAudio nominal device clock is GLOBAL and the
//! last opener wins. If another client (test battery, console, another DAW)
//! leaves the interface clocked at 48 kHz and the app then opens its
//! streams at the configured 44.1 kHz, CoreAudio inserts rate conversion —
//! callbacks arrive on the DEVICE's cycle, not the requested one, and the
//! observed period doubles/floats (the owner's log: `period 2902us` where
//! 44.1 kHz / 64 frames should be 1451 us).
//!
//! Sequence: clock the default input device at 48 kHz (open + drop a
//! stream), then open the owner's shape (44.1 kHz / 64) and histogram the
//! granted callback sizes AND inter-callback gaps.
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use hw_harness::hw_tests_enabled;

fn open_and_histogram(
    device: &cpal::Device,
    channels: u16,
    sample_rate: u32,
    secs: u64,
) -> (Vec<usize>, Vec<u64>) {
    use cpal::traits::{DeviceTrait, StreamTrait};
    let config = cpal::StreamConfig {
        channels,
        sample_rate,
        buffer_size: cpal::BufferSize::Fixed(64),
    };
    let seen = std::sync::Arc::new(std::sync::Mutex::new((
        Vec::<usize>::new(),
        Vec::<u64>::new(),
        None::<std::time::Instant>,
    )));
    let seen_cb = std::sync::Arc::clone(&seen);
    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _: &_| {
                let now = std::time::Instant::now();
                let mut s = seen_cb.lock().unwrap();
                if s.0.len() < 200 {
                    s.0.push(data.len() / channels as usize);
                    if let Some(prev) = s.2 {
                        s.1.push(now.duration_since(prev).as_micros() as u64);
                    }
                    s.2 = Some(now);
                }
            },
            |err| eprintln!("[#698 CLOCK] stream error: {err}"),
            None,
        )
        .expect("build input stream");
    stream.play().expect("play");
    std::thread::sleep(std::time::Duration::from_secs(secs));
    drop(stream);
    let s = seen.lock().unwrap();
    (s.0.clone(), s.1.clone())
}

#[test]
fn device_clocked_at_48k_honors_a_44k_64frame_request() {
    if !hw_tests_enabled("device_clocked_at_48k_honors_a_44k_64frame_request") {
        return;
    }
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    let device = host.default_input_device().expect("default input device");
    let supported = {
        use cpal::traits::DeviceTrait;
        device
            .supported_input_configs()
            .expect("supported configs")
            .next()
            .expect("at least one config")
    };
    let channels = supported.channels();

    // Step 1: clock the device at 48 kHz, like the hw battery / console do.
    let _ = open_and_histogram(&device, channels, 48_000, 1);

    // Step 2: immediately open the owner's shape: 44.1 kHz / 64 frames.
    let (frames, gaps_us) = open_and_histogram(&device, channels, 44_100, 3);

    let mut frame_hist = std::collections::BTreeMap::new();
    for f in &frames {
        *frame_hist.entry(*f).or_insert(0u32) += 1;
    }
    let mut gap_hist = std::collections::BTreeMap::new();
    for g in &gaps_us {
        // 250 us buckets to make the cadence readable.
        *gap_hist.entry(g / 250 * 250).or_insert(0u32) += 1;
    }
    eprintln!(
        "[#698 CLOCK] after 48k clocking, 44.1k/64 request → callback frames \
         {frame_hist:?}, inter-callback gaps (us, 250-bucketed) {gap_hist:?}"
    );

    assert!(!frames.is_empty(), "no callbacks fired");
    let expected_period_us = 64_000_000 / 44_100; // 1451 us
    let off_cadence = gaps_us
        .iter()
        .filter(|&&g| g > expected_period_us * 3 / 2)
        .count();
    assert!(
        frames.iter().all(|&f| f == 64) && off_cadence < gaps_us.len() / 10,
        "BUG #698: with the device clocked at 48 kHz, a 44.1 kHz / 64-frame \
         request is NOT honored on its own cadence — frames {frame_hist:?}, \
         {off_cadence}/{} gaps beyond 1.5x the 1451 us period ({gap_hist:?}). \
         The app's worker then runs on a doubled/floating period (the owner's \
         2902 us) with rate conversion burning extra CPU.",
        gaps_us.len(),
    );
}
