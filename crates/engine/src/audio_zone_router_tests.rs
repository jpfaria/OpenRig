//! The audio's allocations land in their own zone, everything
//! else stays in the system zone, and frees go back where they came from.

use super::{install, is_audio_allocation};
use crate::audio_alloc_scope::audio_allocations;
use crate::audio_zone_regions::audio_zone_bytes_in_use;

extern "C" {
    fn malloc(size: usize) -> *mut u8;
    fn free(ptr: *mut u8);
}

#[test]
fn the_router_installs_on_macos() {
    assert!(install(), "macOS must accept the audio zone router");
    assert!(install(), "a second install is a no-op that still succeeds");
}

#[test]
fn an_allocation_inside_an_audio_scope_lands_in_the_audio_zone() {
    assert!(install());
    let audio = {
        let _scope = audio_allocations();
        vec![1u8; 64 << 10]
    };
    let other = vec![1u8; 64 << 10];
    assert!(
        is_audio_allocation(audio.as_ptr()),
        "a buffer built for the audio must be audio memory"
    );
    assert!(
        !is_audio_allocation(other.as_ptr()),
        "a buffer built outside the audio must stay in the system zone"
    );
}

#[test]
fn a_plugins_own_malloc_is_routed_too() {
    assert!(install());
    let ptr = {
        let _scope = audio_allocations();
        unsafe { malloc(4096) }
    };
    let routed = is_audio_allocation(ptr);
    unsafe { free(ptr) };
    assert!(
        routed,
        "C/C++ plugin code calls malloc directly; it must reach the audio zone"
    );
}

#[test]
fn every_size_class_is_routed() {
    assert!(install());
    let _scope = audio_allocations();
    for size in [16usize, 1 << 10, 64 << 10, 1 << 20, 32 << 20] {
        let buffer = vec![1u8; size];
        assert!(
            is_audio_allocation(buffer.as_ptr()),
            "a {size}-byte audio buffer left the audio zone"
        );
    }
}

#[test]
fn audio_memory_freed_anywhere_goes_back_to_the_audio_zone() {
    assert!(install());
    let size = 24 << 20;
    let before = audio_zone_bytes_in_use();
    let buffer = {
        let _scope = audio_allocations();
        vec![1u8; size]
    };
    assert!(
        audio_zone_bytes_in_use() >= before + size,
        "the audio zone must account the buffer"
    );
    std::thread::spawn(move || drop(buffer)).join().unwrap();
    assert!(
        audio_zone_bytes_in_use() < before + size,
        "a free from another thread must return the buffer to the audio zone"
    );
}

#[test]
fn threads_allocating_both_kinds_at_once_never_mix_them() {
    assert!(install());
    std::thread::scope(|threads| {
        for worker in 0..8 {
            threads.spawn(move || {
                for round in 0..20_000usize {
                    let audio = (round + worker) % 2 == 0;
                    let size = 16 + (round * 37) % 9_000;
                    let mut buffer = if audio {
                        let _scope = audio_allocations();
                        vec![round as u8; size]
                    } else {
                        vec![round as u8; size]
                    };
                    buffer.resize(size * 2, 0);
                    assert_eq!(is_audio_allocation(buffer.as_ptr()), audio);
                }
            });
        }
    });
}
